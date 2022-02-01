#![allow(unused_variables)]

// use cranelift_wasm::{ModuleEnvironment, FuncTranslator};
use cranelift_codegen::entity::{EntityRef, PrimaryMap};
use cranelift_codegen::ir;
use cranelift_codegen::isa::{CallConv, TargetFrontendConfig};
use cranelift_wasm as wasm;
use cranelift_wasm::{DefinedFuncIndex, FuncIndex, TargetEnvironment, TypeIndex, WasmType};

/// Compute a `ir::ExternalName` for a given wasm function index.
fn get_func_name(func_index: FuncIndex) -> ir::ExternalName {
    ir::ExternalName::user(0, func_index.as_u32())
}

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

struct ModuleInfo {
    /// TypeID -> Type
    fun_types: PrimaryMap<TypeIndex, ir::Signature>,
    /// FunID -> TypeID
    funs: PrimaryMap<FuncIndex, Exportable<TypeIndex>>,
    /// Function bodies
    fun_bodies: PrimaryMap<DefinedFuncIndex, ir::Function>,
}

pub struct ModuleEnvironment {
    info: ModuleInfo,
    target_config: TargetFrontendConfig,
}

impl ModuleEnvironment {
    pub fn new(target_config: TargetFrontendConfig) -> Self {
        let info = ModuleInfo {
            fun_types: PrimaryMap::new(),
            funs: PrimaryMap::new(),
            fun_bodies: PrimaryMap::new(),
        };

        Self {
            info,
            target_config,
        }
    }

    fn get_fun_env(&self) -> FunctionEnvironment {
        FunctionEnvironment {
            target_config: self.target_config,
        }
    }

    fn get_fun_type(&self, fun_index: FuncIndex) -> &ir::Signature {
        let type_idx = self.info.funs[fun_index].entity;
        &self.info.fun_types[type_idx]
    }
}

impl TargetEnvironment for ModuleEnvironment {
    fn target_config(&self) -> TargetFrontendConfig {
        self.target_config
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
        let mut sig = ir::Signature::new(CallConv::Fast);
        sig.params
            .extend(wasm_func_type.params().iter().map(&mut wasm_to_ir));
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
        todo!()
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
        validator: wasm::wasmparser::FuncValidator<wasm::wasmparser::ValidatorResources>,
        body: wasm::wasmparser::FunctionBody<'data>,
    ) -> wasm::WasmResult<()> {
        let func_env = self.get_fun_env();
        let func_index = FuncIndex::new(self.info.fun_bodies.len());
        let name = get_func_name(func_index);
        let sig = self.get_fun_type(func_index);
        let func = ir::Function::with_name_signature(name, sig.clone());
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

struct FunctionEnvironment {
    target_config: TargetFrontendConfig,
}

impl wasm::TargetEnvironment for FunctionEnvironment {
    fn target_config(&self) -> TargetFrontendConfig {
        self.target_config
    }
}

impl wasm::FuncEnvironment for FunctionEnvironment {
    fn make_global(
        &mut self,
        func: &mut cranelift_codegen::ir::Function,
        index: wasm::GlobalIndex,
    ) -> wasm::WasmResult<wasm::GlobalVariable> {
        todo!()
    }

    fn make_heap(
        &mut self,
        func: &mut cranelift_codegen::ir::Function,
        index: wasm::MemoryIndex,
    ) -> wasm::WasmResult<cranelift_codegen::ir::Heap> {
        todo!()
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
        todo!()
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
