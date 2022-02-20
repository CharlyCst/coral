#![allow(unused_variables)]

// use cranelift_wasm::{ModuleEnvironment, FuncTranslator};
use cranelift_codegen::cursor;
use cranelift_codegen::entity::{EntityRef, PrimaryMap};
use cranelift_codegen::ir;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::isa::{CallConv, TargetFrontendConfig};
use cranelift_wasm as wasm;
use cranelift_wasm::{DefinedFuncIndex, FuncIndex, TargetEnvironment, TypeIndex, WasmType};

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

pub struct ModuleInfo {
    /// TypeID -> Type
    pub fun_types: PrimaryMap<TypeIndex, ir::Signature>,
    /// FunID -> TypeID
    pub funs: PrimaryMap<FuncIndex, Exportable<TypeIndex>>,
    /// Function bodies
    pub fun_bodies: PrimaryMap<DefinedFuncIndex, (ir::Function, FuncIndex)>,
    /// The registered memories
    pub memories: Vec<wasm::Memory>,
    // Configuration of the target
    target_config: TargetFrontendConfig,
}

impl ModuleInfo {
    fn get_func_sig(&self, fun_index: FuncIndex) -> &ir::Signature {
        let type_idx = self.funs[fun_index].entity;
        &self.fun_types[type_idx]
    }

    fn get_fun_env(&self) -> FunctionEnvironment {
        FunctionEnvironment {
            target_config: self.target_config,
            info: self,
            vmctx: None,
        }
    }
}

pub struct ModuleEnvironment {
    pub info: ModuleInfo,
    translator: wasm::FuncTranslator,
}

impl ModuleEnvironment {
    pub fn new(target_config: TargetFrontendConfig) -> Self {
        let info = ModuleInfo {
            fun_types: PrimaryMap::new(),
            funs: PrimaryMap::new(),
            fun_bodies: PrimaryMap::new(),
            memories: Vec::new(),
            target_config,
        };

        Self {
            info,
            translator: wasm::FuncTranslator::new(),
        }
    }
}

impl TargetEnvironment for ModuleEnvironment {
    fn target_config(&self) -> TargetFrontendConfig {
        self.info.target_config
    }
}

impl<'data> wasm::ModuleEnvironment<'data> for ModuleEnvironment {
    fn declare_type_func(&mut self, wasm_func_type: wasm::WasmFuncType) -> wasm::WasmResult<()> {
        // A small type conversion function
        let mut wasm_to_ir = |ty: &WasmType| {
            let reference_type = match self.pointer_type() {
                ir::types::I32 => ir::types::R32,
                ir::types::I64 => ir::types::R64,
                _ => panic!("unsupported pointer type"),
            };
            ir::AbiParam::new(match ty {
                WasmType::I32 => ir::types::I32,
                WasmType::I64 => ir::types::I64,
                WasmType::F32 => ir::types::F32,
                WasmType::F64 => ir::types::F64,
                WasmType::V128 => ir::types::I8X16,
                WasmType::FuncRef | WasmType::ExternRef | WasmType::ExnRef => reference_type,
            })
        };
        let mut sig = ir::Signature::new(CallConv::SystemV);
        sig.params
            .extend(wasm_func_type.params().iter().map(&mut wasm_to_ir));
        sig.params.push(ir::AbiParam::special(
            self.pointer_type(),
            ir::ArgumentPurpose::VMContext,
        ));
        sig.returns
            .extend(wasm_func_type.returns().iter().map(&mut wasm_to_ir));
        self.info.fun_types.push(sig);
        Ok(())
    }

    fn declare_func_import(
        &mut self,
        index: wasm::TypeIndex,
        module: &'data str,
        field: Option<&'data str>,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_table_import(
        &mut self,
        table: wasm::Table,
        module: &'data str,
        field: Option<&'data str>,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_memory_import(
        &mut self,
        memory: wasm::Memory,
        module: &'data str,
        field: Option<&'data str>,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_global_import(
        &mut self,
        global: wasm::Global,
        module: &'data str,
        field: Option<&'data str>,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_func_type(&mut self, index: wasm::TypeIndex) -> wasm::WasmResult<()> {
        self.info.funs.push(Exportable::new(index));
        Ok(())
    }

    fn declare_table(&mut self, table: wasm::Table) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_memory(&mut self, memory: wasm::Memory) -> wasm::WasmResult<()> {
        self.info.memories.push(memory);
        Ok(())
    }

    fn declare_global(&mut self, global: wasm::Global) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_func_export(
        &mut self,
        func_index: wasm::FuncIndex,
        name: &'data str,
    ) -> wasm::WasmResult<()> {
        self.info.funs[func_index].export_as(name.to_string());
        Ok(())
    }

    fn declare_table_export(
        &mut self,
        table_index: wasm::TableIndex,
        name: &'data str,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_memory_export(
        &mut self,
        memory_index: wasm::MemoryIndex,
        name: &'data str,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_global_export(
        &mut self,
        global_index: wasm::GlobalIndex,
        name: &'data str,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_start_func(&mut self, index: wasm::FuncIndex) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_table_elements(
        &mut self,
        table_index: wasm::TableIndex,
        base: Option<wasm::GlobalIndex>,
        offset: u32,
        elements: Box<[wasm::FuncIndex]>,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_passive_element(
        &mut self,
        index: wasm::ElemIndex,
        elements: Box<[wasm::FuncIndex]>,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn declare_passive_data(
        &mut self,
        data_index: wasm::DataIndex,
        data: &'data [u8],
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn define_function_body(
        &mut self,
        mut validator: wasm::wasmparser::FuncValidator<wasm::wasmparser::ValidatorResources>,
        body: wasm::wasmparser::FunctionBody<'data>,
    ) -> wasm::WasmResult<()> {
        let mut fun_env = self.info.get_fun_env();
        let func_index = FuncIndex::new(self.info.fun_bodies.len());
        let name = get_func_name(func_index);
        let sig = self.info.get_func_sig(func_index);
        let mut fun = ir::Function::with_name_signature(name, sig.clone());
        self.translator
            .translate_body(&mut validator, body, &mut fun, &mut fun_env)?;
        self.info.fun_bodies.push((fun, func_index));
        Ok(())
    }

    fn declare_data_initialization(
        &mut self,
        memory_index: wasm::MemoryIndex,
        base: Option<wasm::GlobalIndex>,
        offset: u64,
        data: &'data [u8],
    ) -> wasm::WasmResult<()> {
        todo!()
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

impl<'info> wasm::TargetEnvironment for FunctionEnvironment<'info> {
    fn target_config(&self) -> TargetFrontendConfig {
        self.target_config
    }
}

impl<'info> wasm::FuncEnvironment for FunctionEnvironment<'info> {
    fn make_global(
        &mut self,
        func: &mut ir::Function,
        index: wasm::GlobalIndex,
    ) -> wasm::WasmResult<wasm::GlobalVariable> {
        todo!()
    }

    fn make_heap(
        &mut self,
        func: &mut ir::Function,
        index: wasm::MemoryIndex,
    ) -> wasm::WasmResult<ir::Heap> {
        // Heaps addresses are stored in the VMContext
        println!("Heap: {:?}", func);
        let vmctx = self.vmctx(func);
        let base = func.create_global_value(ir::GlobalValueData::Load {
            base: vmctx,
            offset: 0.into(), // TODO: retrieve memory offset
            global_type: ir::types::I64,
            readonly: true, // TODO: readonly if the heap is static
        });
        let heap = func.create_heap(ir::HeapData {
            base,
            min_size: 1.into(),
            offset_guard_size: 0.into(),
            style: ir::HeapStyle::Static { bound: 1.into() },
            index_type: ir::types::I32,
        });
        Ok(heap)
    }

    fn make_table(
        &mut self,
        func: &mut cranelift_codegen::ir::Function,
        index: wasm::TableIndex,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Table> {
        todo!()
    }

    fn make_indirect_sig(
        &mut self,
        func: &mut cranelift_codegen::ir::Function,
        index: TypeIndex,
    ) -> wasm::WasmResult<cranelift_codegen::ir::SigRef> {
        todo!()
    }

    fn make_direct_func(
        &mut self,
        func: &mut cranelift_codegen::ir::Function,
        index: FuncIndex,
    ) -> wasm::WasmResult<cranelift_codegen::ir::FuncRef> {
        let name = get_func_name(index);
        let signature = self.info.get_func_sig(index);
        // TODO: can we somehow avoid cloning here? Maybe keep a map of SigRef somewhere.
        let signature = func.import_signature(signature.clone());
        Ok(func.import_function(ir::ExtFuncData {
            name,
            signature,
            colocated: false, // TODO: set that to true if the func lives in the same module
        }))
    }

    fn translate_call(
        &mut self,
        mut pos: cursor::FuncCursor,
        _callee_index: FuncIndex,
        callee: ir::FuncRef,
        call_args: &[ir::Value],
    ) -> wasm::WasmResult<ir::Inst> {
        // Append the vmctx to the call arguments
        let caller_vmctx = pos
            .func
            .special_param(ir::ArgumentPurpose::VMContext)
            .unwrap();
        let mut real_call_args = Vec::with_capacity(call_args.len() + 1);
        real_call_args.extend(call_args);
        real_call_args.push(caller_vmctx);
        Ok(pos.ins().call(callee, &real_call_args))
    }

    fn translate_call_indirect(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        table_index: wasm::TableIndex,
        table: cranelift_codegen::ir::Table,
        sig_index: TypeIndex,
        sig_ref: cranelift_codegen::ir::SigRef,
        callee: cranelift_codegen::ir::Value,
        call_args: &[cranelift_codegen::ir::Value],
    ) -> wasm::WasmResult<cranelift_codegen::ir::Inst> {
        todo!()
    }

    fn translate_memory_grow(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        index: wasm::MemoryIndex,
        heap: cranelift_codegen::ir::Heap,
        val: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_memory_size(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        index: wasm::MemoryIndex,
        heap: cranelift_codegen::ir::Heap,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_memory_copy(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        src_index: wasm::MemoryIndex,
        src_heap: cranelift_codegen::ir::Heap,
        dst_index: wasm::MemoryIndex,
        dst_heap: cranelift_codegen::ir::Heap,
        dst: cranelift_codegen::ir::Value,
        src: cranelift_codegen::ir::Value,
        len: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_memory_fill(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        index: wasm::MemoryIndex,
        heap: cranelift_codegen::ir::Heap,
        dst: cranelift_codegen::ir::Value,
        val: cranelift_codegen::ir::Value,
        len: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_memory_init(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        index: wasm::MemoryIndex,
        heap: cranelift_codegen::ir::Heap,
        seg_index: u32,
        dst: cranelift_codegen::ir::Value,
        src: cranelift_codegen::ir::Value,
        len: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_data_drop(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        seg_index: u32,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_table_size(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        index: wasm::TableIndex,
        table: cranelift_codegen::ir::Table,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_table_grow(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        table_index: wasm::TableIndex,
        table: cranelift_codegen::ir::Table,
        delta: cranelift_codegen::ir::Value,
        init_value: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_table_get(
        &mut self,
        builder: &mut wasm::FunctionBuilder,
        table_index: wasm::TableIndex,
        table: cranelift_codegen::ir::Table,
        index: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_table_set(
        &mut self,
        builder: &mut wasm::FunctionBuilder,
        table_index: wasm::TableIndex,
        table: cranelift_codegen::ir::Table,
        value: cranelift_codegen::ir::Value,
        index: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_table_copy(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        dst_table_index: wasm::TableIndex,
        dst_table: cranelift_codegen::ir::Table,
        src_table_index: wasm::TableIndex,
        src_table: cranelift_codegen::ir::Table,
        dst: cranelift_codegen::ir::Value,
        src: cranelift_codegen::ir::Value,
        len: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_table_fill(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        table_index: wasm::TableIndex,
        dst: cranelift_codegen::ir::Value,
        val: cranelift_codegen::ir::Value,
        len: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_table_init(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        seg_index: u32,
        table_index: wasm::TableIndex,
        table: cranelift_codegen::ir::Table,
        dst: cranelift_codegen::ir::Value,
        src: cranelift_codegen::ir::Value,
        len: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_elem_drop(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        seg_index: u32,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_ref_func(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        func_index: FuncIndex,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_custom_global_get(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        global_index: wasm::GlobalIndex,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_custom_global_set(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        global_index: wasm::GlobalIndex,
        val: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<()> {
        todo!()
    }

    fn translate_atomic_wait(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        index: wasm::MemoryIndex,
        heap: cranelift_codegen::ir::Heap,
        addr: cranelift_codegen::ir::Value,
        expected: cranelift_codegen::ir::Value,
        timeout: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn translate_atomic_notify(
        &mut self,
        pos: cranelift_codegen::cursor::FuncCursor,
        index: wasm::MemoryIndex,
        heap: cranelift_codegen::ir::Heap,
        addr: cranelift_codegen::ir::Value,
        count: cranelift_codegen::ir::Value,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Value> {
        todo!()
    }

    fn unsigned_add_overflow_condition(&self) -> cranelift_codegen::ir::condcodes::IntCC {
        todo!()
    }
}
