// #![allow(unused_variables)]

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use cranelift_codegen::cursor;
use cranelift_codegen::ir;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::isa::{CallConv, TargetFrontendConfig};
use cranelift_wasm as cw;

use cranelift_wasm::{
    DefinedFuncIndex, FuncIndex, GlobalIndex, MemoryIndex, TableIndex, TargetEnvironment,
    TypeIndex, WasmType,
};

use collections::{EntityRef, PrimaryMap, SecondaryMap};
use wasm::ImportIndex;

/// Size of a wasm page, defined by the standard.
const WASM_PAGE_SIZE: u64 = 0x10000; // 64 Ki
/// Width of a VMContext entry. For now the width is independent of the architecture, and thorefore
/// each entry span 8 bytes even for 32 bits architectures.
const VMCTX_ENTRY_WIDTH: i32 = 0x8;

/// Compute a `ir::ExternalName` for a given wasm function index.
fn get_func_name(func_index: FuncIndex) -> ir::ExternalName {
    ir::ExternalName::user(0, func_index.as_u32())
}

#[derive(Debug)]
pub struct Exportable<T> {
    /// A wasm entity.
    pub entity: T,

    /// Names under which the entity is exported.
    pub export_names: Vec<String>,
}

impl<T> Exportable<T> {
    pub fn new(entity: T) -> Self {
        Self {
            entity,
            export_names: Vec::new(),
        }
    }

    pub fn export_as(&mut self, name: String) {
        self.export_names.push(name);
    }
}

#[derive(Clone)]
pub struct ImportedFunc {
    /// The index of the module.
    pub module: ImportIndex,
    /// The name of the imported function inside its module.
    pub name: String,
    /// Index of the function in the VMContext.
    pub vmctx_idx: i32,
}

#[derive(Clone)]
pub struct ImportedHeap {
    /// The index of the module
    pub module: ImportIndex,
    /// The name of the imported heap inside its module.
    pub name: String,
}

#[derive(Clone)]
pub struct ImportedGlob {
    /// The index of the module.
    pub module: ImportIndex,
    /// The name of the imported global inside its module.
    pub name: String,
}

#[derive(Clone)]
pub struct ImportedTable {
    /// The index of the module.
    pub module: ImportIndex,
    /// The name of the imported global inside its module.
    pub name: String,
}

#[derive(Clone)]
pub struct DataSegment {
    /// The memory to which the segment must be applied.
    pub memory_index: MemoryIndex,
    /// An optional base, in the form of a global.
    pub base: Option<GlobalIndex>,
    /// Offset, relative to the base if any, to 0 otherwise.
    pub offset: u64,
    /// The actual data.
    pub data: Vec<u8>,
}

#[derive(Clone)]
pub struct TableSegment {
    /// The table to which the segment must be applied.
    pub table_index: TableIndex,
    /// An optional base, in the form of a global.
    pub base: Option<GlobalIndex>,
    /// Offset, relative to the base if any, to 0 otherwise.
    pub offset: u32,
    /// The actual elements
    pub elements: Box<[FuncIndex]>,
}

pub struct ModuleInfo {
    /// FunID -> TypeID
    pub funcs: PrimaryMap<FuncIndex, Exportable<TypeIndex>>,
    /// TypeID -> Wasm Type
    pub types: PrimaryMap<TypeIndex, cw::WasmFuncType>,
    /// TypeID -> Cranelift Signature
    pub func_signatures: SecondaryMap<TypeIndex, Option<ir::Signature>>,
    /// FunID -> Option<imported_func_info>
    pub imported_funcs: SecondaryMap<FuncIndex, Option<ImportedFunc>>,
    /// Function bodies
    pub func_bodies: PrimaryMap<DefinedFuncIndex, (ir::Function, FuncIndex)>,
    /// The registered memories
    pub heaps: PrimaryMap<MemoryIndex, Exportable<cw::Memory>>,
    /// A mapping MemoryID -> imported_heap_info
    pub imported_heaps: SecondaryMap<MemoryIndex, Option<ImportedHeap>>,
    /// The list of globals
    pub globs: PrimaryMap<GlobalIndex, Exportable<cw::Global>>,
    /// A mapping GlobalID -> imported_glob_info
    pub imported_globs: SecondaryMap<GlobalIndex, Option<ImportedGlob>>,
    /// The list of tables.
    pub tables: PrimaryMap<TableIndex, Exportable<cw::Table>>,
    /// A mapping TableID -> imported_table_info
    pub imported_tables: SecondaryMap<TableIndex, Option<ImportedTable>>,
    /// The list of imported modules
    pub modules: PrimaryMap<ImportIndex, String>,
    /// The list of data segments to initialize.
    pub segments: Vec<DataSegment>,
    /// The list of table elements to initialize.
    pub elements: Vec<TableSegment>,
    /// The start function, to be called after memory and table initialization.
    pub start: Option<FuncIndex>,
    /// The number of imported funcs. The defined functions goes after the imported ones.
    nb_imported_funcs: usize,
    /// Configuration of the target
    target_config: TargetFrontendConfig,
}

impl ModuleInfo {
    fn get_func_sig(&self, fun_index: FuncIndex) -> &ir::Signature {
        let type_idx = self.funcs[fun_index].entity;
        self.func_signatures[type_idx].as_ref().unwrap()
    }

    fn get_fun_env(&self) -> FunctionEnvironment {
        FunctionEnvironment {
            target_config: self.target_config,
            info: self,
            vmctx: None,
        }
    }

    /// Return the index of a module. The module is registered if it hasn't been seen yet.
    fn get_module_idx(&mut self, module: &str) -> ImportIndex {
        // TODO: we might want to get rid of the linear scan here
        if let Some((idx, _)) = self
            .modules
            .iter()
            .find(|(_, known_module)| module == known_module.as_str())
        {
            idx
        } else {
            self.modules.push(module.to_owned())
        }
    }

    fn get_vmctx_table_offset(&self, table: TableIndex) -> i32 {
        (self.heaps.len() + table.index() * 2) as i32 * VMCTX_ENTRY_WIDTH
    }

    fn get_vmctx_imported_vmctx_offset(&self, module: ImportIndex) -> i32 {
        (self.heaps.len() + self.tables.len() * 2 + self.nb_imported_funcs + module.index()) as i32
            * VMCTX_ENTRY_WIDTH
    }

    fn get_vmctx_global_offset(&self, global: GlobalIndex) -> i32 {
        (self.heaps.len()
            + self.tables.len() * 2
            + self.nb_imported_funcs
            + self.modules.len()
            + global.index()) as i32
            * VMCTX_ENTRY_WIDTH
    }

    /// Translate a wasm type to it's IR representation
    fn wasm_to_ir_type(&self, ty: WasmType) -> ir::Type {
        match ty {
            WasmType::I32 => ir::types::I32,
            WasmType::I64 => ir::types::I64,
            WasmType::F32 => ir::types::F32,
            WasmType::F64 => ir::types::F64,
            WasmType::V128 => ir::types::I8X16,
            WasmType::FuncRef | WasmType::ExternRef => match self.pointer_type() {
                ir::types::I32 => ir::types::R32,
                ir::types::I64 => ir::types::R64,
                _ => panic!("unsupported pointer type"),
            },
        }
    }
}

impl TargetEnvironment for ModuleInfo {
    fn target_config(&self) -> TargetFrontendConfig {
        self.target_config
    }
}

pub struct ModuleEnvironment {
    pub info: ModuleInfo,
    translator: cw::FuncTranslator,
}

impl ModuleEnvironment {
    pub fn new(target_config: TargetFrontendConfig) -> Self {
        let info = ModuleInfo {
            funcs: PrimaryMap::new(),
            types: PrimaryMap::new(),
            func_signatures: SecondaryMap::new(),
            imported_funcs: SecondaryMap::new(),
            func_bodies: PrimaryMap::new(),
            heaps: PrimaryMap::new(),
            imported_heaps: SecondaryMap::new(),
            globs: PrimaryMap::new(),
            imported_globs: SecondaryMap::new(),
            tables: PrimaryMap::new(),
            imported_tables: SecondaryMap::new(),
            modules: PrimaryMap::new(),
            segments: Vec::new(),
            elements: Vec::new(),
            start: None,
            nb_imported_funcs: 0,
            target_config,
        };

        Self {
            info,
            translator: cw::FuncTranslator::new(),
        }
    }
}

impl TargetEnvironment for ModuleEnvironment {
    fn target_config(&self) -> TargetFrontendConfig {
        self.info.target_config
    }
}

impl<'data> cw::ModuleEnvironment<'data> for ModuleEnvironment {
    fn declare_type_func(&mut self, wasm_func_type: cw::WasmFuncType) -> cw::WasmResult<()> {
        // A small type conversion function
        let mut wasm_to_ir = |ty: &WasmType| ir::AbiParam::new(self.info.wasm_to_ir_type(*ty));
        let mut sig = ir::Signature::new(CallConv::SystemV);
        sig.params
            .extend(wasm_func_type.params().iter().map(&mut wasm_to_ir));
        sig.params.push(ir::AbiParam::special(
            self.pointer_type(),
            ir::ArgumentPurpose::VMContext,
        ));
        sig.returns
            .extend(wasm_func_type.returns().iter().map(&mut wasm_to_ir));

        let ty_idx = self.info.types.push(wasm_func_type);
        self.info.func_signatures[ty_idx] = Some(sig);
        Ok(())
    }

    fn declare_func_import(
        &mut self,
        ty_idx: cw::TypeIndex,
        module: &'data str,
        field: &'data str,
    ) -> cw::WasmResult<()> {
        let index = self.info.funcs.push(Exportable::new(ty_idx));
        self.info.nb_imported_funcs += 1;
        let vmctx_idx = self.info.nb_imported_funcs as i32;
        let module_idx = self.info.get_module_idx(module);
        self.info.imported_funcs[index] = Some(ImportedFunc {
            module: module_idx,
            name: field.to_string(),
            vmctx_idx,
        });
        Ok(())
    }

    fn declare_table_import(
        &mut self,
        table: cw::Table,
        module: &'data str,
        field: &'data str,
    ) -> cw::WasmResult<()> {
        let index = self.info.tables.push(Exportable::new(table));
        let module_idx = self.info.get_module_idx(module);
        self.info.imported_tables[index] = Some(ImportedTable {
            module: module_idx,
            name: field.to_string(),
        });
        Ok(())
    }

    fn declare_memory_import(
        &mut self,
        memory: cw::Memory,
        module: &'data str,
        field: &'data str,
    ) -> cw::WasmResult<()> {
        let index = self.info.heaps.push(Exportable::new(memory));
        let module_idx = self.info.get_module_idx(module);
        self.info.imported_heaps[index] = Some(ImportedHeap {
            module: module_idx,
            name: field.to_string(),
        });
        Ok(())
    }

    fn declare_global_import(
        &mut self,
        global: cw::Global,
        module: &'data str,
        field: &'data str,
    ) -> cw::WasmResult<()> {
        let index = self.info.globs.push(Exportable::new(global));
        let module_idx = self.info.get_module_idx(module);
        // TODO: what if we didn't parse all function declaration yet, is that still correct?
        self.info.imported_globs[index] = Some(ImportedGlob {
            module: module_idx,
            name: field.to_string(),
        });
        Ok(())
    }

    fn declare_func_type(&mut self, ty_idx: cw::TypeIndex) -> cw::WasmResult<()> {
        self.info.funcs.push(Exportable::new(ty_idx));
        Ok(())
    }

    fn declare_table(&mut self, table: cw::Table) -> cw::WasmResult<()> {
        self.info.tables.push(Exportable::new(table));
        Ok(())
    }

    fn declare_memory(&mut self, memory: cw::Memory) -> cw::WasmResult<()> {
        self.info.heaps.push(Exportable::new(memory));
        Ok(())
    }

    fn declare_global(&mut self, global: cw::Global) -> cw::WasmResult<()> {
        self.info.globs.push(Exportable::new(global));
        Ok(())
    }

    fn declare_func_export(
        &mut self,
        func_index: cw::FuncIndex,
        name: &'data str,
    ) -> cw::WasmResult<()> {
        self.info.funcs[func_index].export_as(name.to_string());
        Ok(())
    }

    fn declare_table_export(
        &mut self,
        table_index: cw::TableIndex,
        name: &'data str,
    ) -> cw::WasmResult<()> {
        self.info.tables[table_index].export_as(name.to_string());
        Ok(())
    }

    fn declare_memory_export(
        &mut self,
        memory_index: cw::MemoryIndex,
        name: &'data str,
    ) -> cw::WasmResult<()> {
        self.info.heaps[memory_index].export_as(name.to_string());
        Ok(())
    }

    fn declare_global_export(
        &mut self,
        global_index: cw::GlobalIndex,
        name: &'data str,
    ) -> cw::WasmResult<()> {
        self.info.globs[global_index].export_as(name.to_string());
        Ok(())
    }

    fn declare_start_func(&mut self, index: cw::FuncIndex) -> cw::WasmResult<()> {
        self.info.start = Some(index);
        Ok(())
    }

    fn declare_table_elements(
        &mut self,
        table_index: TableIndex,
        base: Option<GlobalIndex>,
        offset: u32,
        elements: Box<[FuncIndex]>,
    ) -> cw::WasmResult<()> {
        let segment = TableSegment {
            table_index,
            base,
            offset,
            elements,
        };
        self.info.elements.push(segment);
        Ok(())
    }

    fn declare_passive_element(
        &mut self,
        _index: cw::ElemIndex,
        _elements: Box<[cw::FuncIndex]>,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn declare_passive_data(
        &mut self,
        _data_index: cw::DataIndex,
        _data: &'data [u8],
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn define_function_body(
        &mut self,
        mut validator: cw::wasmparser::FuncValidator<cw::wasmparser::ValidatorResources>,
        body: cw::wasmparser::FunctionBody<'data>,
    ) -> cw::WasmResult<()> {
        let mut fun_env = self.info.get_fun_env();
        // the local functions are declared after the imported ones, and the declaration order is
        // the same for the functions and their bodies.
        let func_index = FuncIndex::new(self.info.nb_imported_funcs + self.info.func_bodies.len());
        let name = get_func_name(func_index);
        let sig = self.info.get_func_sig(func_index);
        let mut fun = ir::Function::with_name_signature(name, sig.clone());
        self.translator
            .translate_body(&mut validator, body, &mut fun, &mut fun_env)?;
        self.info.func_bodies.push((fun, func_index));
        Ok(())
    }

    fn declare_data_initialization(
        &mut self,
        memory_index: MemoryIndex,
        base: Option<GlobalIndex>,
        offset: u64,
        data: &'data [u8],
    ) -> cw::WasmResult<()> {
        let data_segment = DataSegment {
            memory_index,
            base,
            offset,
            data: data.to_vec(),
        };
        self.info.segments.push(data_segment);
        Ok(())
    }
}

struct FunctionEnvironment<'info> {
    target_config: TargetFrontendConfig,
    info: &'info ModuleInfo,

    /// A global variable containing the VMContext
    vmctx: Option<ir::GlobalValue>,
}

impl<'info> FunctionEnvironment<'info> {
    fn vmctx(&mut self, func: &mut ir::Function) -> ir::GlobalValue {
        if let Some(vmctx) = self.vmctx {
            vmctx
        } else {
            let vmctx = func.create_global_value(ir::GlobalValueData::VMContext);
            self.vmctx = Some(vmctx);
            vmctx
        }
    }
}

impl<'info> cw::TargetEnvironment for FunctionEnvironment<'info> {
    fn target_config(&self) -> TargetFrontendConfig {
        self.target_config
    }
}

impl<'info> cw::FuncEnvironment for FunctionEnvironment<'info> {
    fn make_global(
        &mut self,
        func: &mut ir::Function,
        index: cw::GlobalIndex,
    ) -> cw::WasmResult<cw::GlobalVariable> {
        // There are two kinds of globals: locally defined and imported globals.
        // - Locally defined globals are stored in the VMContext.
        // - Imported globals are stored in a foreign VMContext but are pointed to by an entry
        //   in the local VMContext.
        let vmctx = self.vmctx(func);
        let offset = self.info.get_vmctx_global_offset(index).into();
        let global = self.info.globs[index].entity;
        let ty = self.info.wasm_to_ir_type(global.wasm_ty);
        if self.info.imported_globs[index].is_some() {
            let global_ptr = func.create_global_value(ir::GlobalValueData::Load {
                base: vmctx,
                offset,
                global_type: self.pointer_type(),
                readonly: false, // We might want to support hot swapping in the future
            });
            Ok(cw::GlobalVariable::Memory {
                gv: global_ptr,
                offset: 0.into(), // Directly pointed to
                ty,
            })
        } else {
            Ok(cw::GlobalVariable::Memory {
                gv: vmctx,
                offset,
                ty,
            })
        }
    }

    fn make_heap(
        &mut self,
        func: &mut ir::Function,
        index: cw::MemoryIndex,
    ) -> cw::WasmResult<ir::Heap> {
        // Retrieve the memory bound
        // TODO: handle resizeable heaps
        let memory = &self.info.heaps[index].entity;
        let bound = memory.minimum * WASM_PAGE_SIZE;

        // Heaps addresses are stored in the VMContext
        let vmctx = self.vmctx(func);
        let base = func.create_global_value(ir::GlobalValueData::Load {
            base: vmctx,
            offset: 0.into(), // TODO: retrieve memory offset
            global_type: self.pointer_type(),
            readonly: false, // TODO: readonly if the heap is static
        });
        let heap = func.create_heap(ir::HeapData {
            base,
            min_size: WASM_PAGE_SIZE.into(),
            offset_guard_size: 0.into(),
            style: ir::HeapStyle::Static {
                bound: bound.into(),
            },
            index_type: ir::types::I32, // TODO: handle wasm64
        });
        Ok(heap)
    }

    fn make_table(
        &mut self,
        func: &mut ir::Function,
        index: cw::TableIndex,
    ) -> cw::WasmResult<ir::Table> {
        let pointer_type = self.pointer_type();
        let table = &self.info.tables[index].entity;
        let reference_type = self.reference_type(table.wasm_ty);
        let vmctx = self.vmctx(func);
        let offset = self.info.get_vmctx_table_offset(index);

        let base = func.create_global_value(ir::GlobalValueData::Load {
            base: vmctx,
            offset: offset.into(),
            global_type: pointer_type,
            readonly: false,
        });
        let bound = func.create_global_value(ir::GlobalValueData::Load {
            base: vmctx,
            offset: (offset + VMCTX_ENTRY_WIDTH).into(),
            global_type: ir::types::I32,
            readonly: false,
        });
        Ok(func.create_table(ir::TableData {
            base_gv: base,
            min_size: (table.minimum as u64).into(),
            bound_gv: bound,
            element_size: (reference_type.bytes() as u64).into(),
            index_type: ir::types::I32,
        }))
    }

    fn make_indirect_sig(
        &mut self,
        _func: &mut cranelift_codegen::ir::Function,
        _index: TypeIndex,
    ) -> cw::WasmResult<cranelift_codegen::ir::SigRef> {
        todo!()
    }

    fn make_direct_func(
        &mut self,
        func: &mut cranelift_codegen::ir::Function,
        index: FuncIndex,
    ) -> cw::WasmResult<cranelift_codegen::ir::FuncRef> {
        let name = get_func_name(index);
        let signature = self.info.get_func_sig(index);
        // TODO: can we somehow avoid cloning here? Maybe keep a map of SigRef somewhere.
        let signature = func.import_signature(signature.clone());
        Ok(func.import_function(ir::ExtFuncData {
            name,
            signature,
            colocated: self.info.imported_funcs[index].is_none(),
        }))
    }

    fn translate_call(
        &mut self,
        mut pos: cursor::FuncCursor,
        callee_idx: FuncIndex,
        callee: ir::FuncRef,
        call_args: &[ir::Value],
    ) -> cw::WasmResult<ir::Inst> {
        // There is a distinction for functions defined inside and outside the module.
        // Functions defined inside can be called directly, whereas the context must be changed for
        // functions defined outside.
        if let Some(func) = &self.info.imported_funcs[callee_idx] {
            // Indirect call
            let vmctx = self.vmctx(pos.func);
            let vmctx_offset = self.info.get_vmctx_imported_vmctx_offset(func.module);
            // NOTE: we could use the following address for a relative call, which would remove the
            // need for inter-module relocations and therefore allow code sharing.
            //
            // let func_addr = pos.func.create_global_value(ir::GlobalValueData::Load {
            //     base: vmctx,
            //     offset: func_offset.into(),
            //     global_type: self.pointer_type(),
            //     readonly: false, // Because we might want to support hot swapping in the future
            // });
            let callee_vmctx = pos.func.create_global_value(ir::GlobalValueData::Load {
                base: vmctx,
                offset: vmctx_offset.into(),
                global_type: self.pointer_type(),
                readonly: false, // Because we might want to support hot swapping in the future
            });
            let callee_vmctx = pos.ins().global_value(self.pointer_type(), callee_vmctx);

            // Append the called module's vmctx to the call arguments
            let mut real_call_args = Vec::with_capacity(call_args.len() + 1);
            real_call_args.extend(call_args);
            real_call_args.push(callee_vmctx);
            Ok(pos.ins().call(callee, &real_call_args)) // TODO: use a relative call
        } else {
            // Direct call
            //
            // Append the vmctx to the call arguments and perform a direct call
            let caller_vmctx = pos
                .func
                .special_param(ir::ArgumentPurpose::VMContext)
                .unwrap();
            let mut real_call_args = Vec::with_capacity(call_args.len() + 1);
            real_call_args.extend(call_args);
            real_call_args.push(caller_vmctx);
            Ok(pos.ins().call(callee, &real_call_args))
        }
    }

    fn translate_call_indirect(
        &mut self,
        _pos: &mut cw::FunctionBuilder<'_>,
        _table_index: cw::TableIndex,
        _table: cranelift_codegen::ir::Table,
        _sig_index: TypeIndex,
        _sig_ref: cranelift_codegen::ir::SigRef,
        _callee: cranelift_codegen::ir::Value,
        _call_args: &[cranelift_codegen::ir::Value],
    ) -> cw::WasmResult<cranelift_codegen::ir::Inst> {
        todo!()
    }

    fn translate_memory_grow(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _index: cw::MemoryIndex,
        _heap: cranelift_codegen::ir::Heap,
        _val: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_memory_size(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _index: cw::MemoryIndex,
        _heap: cranelift_codegen::ir::Heap,
    ) -> cw::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_memory_copy(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _src_index: cw::MemoryIndex,
        _src_heap: cranelift_codegen::ir::Heap,
        _dst_index: cw::MemoryIndex,
        _dst_heap: cranelift_codegen::ir::Heap,
        _dst: cranelift_codegen::ir::Value,
        _src: cranelift_codegen::ir::Value,
        _len: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_memory_fill(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _index: cw::MemoryIndex,
        _heap: cranelift_codegen::ir::Heap,
        _dst: cranelift_codegen::ir::Value,
        _val: cranelift_codegen::ir::Value,
        _len: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_memory_init(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _index: cw::MemoryIndex,
        _heap: cranelift_codegen::ir::Heap,
        _seg_index: u32,
        _dst: cranelift_codegen::ir::Value,
        _src: cranelift_codegen::ir::Value,
        _len: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_data_drop(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _seg_index: u32,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_table_size(
        &mut self,
        mut pos: cranelift_codegen::cursor::FuncCursor,
        _index: cw::TableIndex,
        table: cranelift_codegen::ir::Table,
    ) -> cw::WasmResult<cranelift_codegen::ir::Value> {
        let size = pos.func.tables[table].bound_gv;
        Ok(pos.ins().global_value(ir::types::I32, size))
    }

    fn translate_table_grow(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _table_index: cw::TableIndex,
        _table: cranelift_codegen::ir::Table,
        _delta: cranelift_codegen::ir::Value,
        _init_value: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_table_get(
        &mut self,
        builder: &mut cw::FunctionBuilder,
        _table_index: cw::TableIndex,
        table: ir::Table,
        index: ir::Value,
    ) -> cw::WasmResult<ir::Value> {
        // TODO: get the type corresponding to the table!
        let table_type = cw::WasmType::ExternRef; // TODO: change me!
        let pointer_type = self.pointer_type();
        let reference_type = self.reference_type(table_type);

        // Load the element from the table.
        let elem_addr = builder.ins().table_addr(pointer_type, table, index, 0);
        let flags = ir::MemFlags::trusted().with_table();
        let elem = builder.ins().load(reference_type, flags, elem_addr, 0);
        Ok(elem)
    }

    fn translate_table_set(
        &mut self,
        builder: &mut cw::FunctionBuilder,
        _table_index: cw::TableIndex,
        table: cranelift_codegen::ir::Table,
        value: cranelift_codegen::ir::Value,
        index: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<()> {
        let pointer_type = self.pointer_type();

        // Store the element into the table.
        let elem_addr = builder.ins().table_addr(pointer_type, table, index, 0);
        let flags = ir::MemFlags::trusted().with_table();
        builder.ins().store(flags, value, elem_addr, 0);
        Ok(())
    }

    fn translate_table_copy(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _dst_table_index: cw::TableIndex,
        _dst_table: cranelift_codegen::ir::Table,
        _src_table_index: cw::TableIndex,
        _src_table: cranelift_codegen::ir::Table,
        _dst: cranelift_codegen::ir::Value,
        _src: cranelift_codegen::ir::Value,
        _len: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_table_fill(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _table_index: cw::TableIndex,
        _dst: cranelift_codegen::ir::Value,
        _val: cranelift_codegen::ir::Value,
        _len: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_table_init(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _seg_index: u32,
        _table_index: cw::TableIndex,
        _table: cranelift_codegen::ir::Table,
        _dst: cranelift_codegen::ir::Value,
        _src: cranelift_codegen::ir::Value,
        _len: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_elem_drop(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _seg_index: u32,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_ref_func(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _func_index: FuncIndex,
    ) -> cw::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_custom_global_get(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _global_index: cw::GlobalIndex,
    ) -> cw::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_custom_global_set(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _global_index: cw::GlobalIndex,
        _val: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<()> {
        todo!()
    }

    fn translate_atomic_wait(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _index: cw::MemoryIndex,
        _heap: cranelift_codegen::ir::Heap,
        _addr: cranelift_codegen::ir::Value,
        _expected: cranelift_codegen::ir::Value,
        _timeout: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_atomic_notify(
        &mut self,
        _pos: cranelift_codegen::cursor::FuncCursor,
        _index: cw::MemoryIndex,
        _heap: cranelift_codegen::ir::Heap,
        _addr: cranelift_codegen::ir::Value,
        _count: cranelift_codegen::ir::Value,
    ) -> cw::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn unsigned_add_overflow_condition(&self) -> cranelift_codegen::ir::condcodes::IntCC {
        todo!()
    }
}
