use crate::alloc::string::{String, ToString};
use crate::alloc::vec::Vec;

use crate::abi::{ExternRef64, WasmParams, WasmResults, WasmType};
use crate::funcs::NativeFunc;
use crate::traits::{
    DataSegment, FuncIndex, FuncInfo, FuncPtr, GlobIndex, GlobInfo, HeapIndex, HeapInfo,
    ImportIndex, Reloc, TableIndex, TableInfo, TableSegment,
};
use crate::traits::{ItemRef, Module, VMContextLayout};
use crate::{FuncType, RefType, TypeIndex};
use collections::{FrozenMap, HashMap, PrimaryMap};

// —————————————————————————————————— VMCS —————————————————————————————————— //

#[derive(Clone)]
pub struct SimpleVMContextLayout {
    funcs: Vec<FuncIndex>,
    heaps: Vec<HeapIndex>,
    tables: Vec<TableIndex>,
    globs: Vec<GlobIndex>,
    imports: Vec<ImportIndex>,
}

impl SimpleVMContextLayout {
    pub fn new(
        funcs: Vec<FuncIndex>,
        heaps: Vec<HeapIndex>,
        tables: Vec<TableIndex>,
        globs: Vec<GlobIndex>,
        imports: Vec<ImportIndex>,
    ) -> Self {
        Self {
            funcs,
            heaps,
            tables,
            globs,
            imports,
        }
    }
}

impl VMContextLayout for SimpleVMContextLayout {
    fn heaps(&self) -> &[HeapIndex] {
        &self.heaps
    }

    fn tables(&self) -> &[TableIndex] {
        &self.tables
    }

    fn funcs(&self) -> &[FuncIndex] {
        &self.funcs
    }

    fn globs(&self) -> &[GlobIndex] {
        &self.globs
    }

    fn imports(&self) -> &[ImportIndex] {
        &self.imports
    }
}

// —————————————————————————————— Wasm Module ——————————————————————————————— //

pub struct ModuleInfo {
    exported_items: HashMap<String, ItemRef>,
    funcs: FrozenMap<FuncIndex, FuncInfo>,
    types: FrozenMap<TypeIndex, FuncType>,
    heaps: FrozenMap<HeapIndex, HeapInfo>,
    tables: FrozenMap<TableIndex, TableInfo>,
    globs: FrozenMap<GlobIndex, GlobInfo>,
    imports: FrozenMap<ImportIndex, String>,
    segments: Vec<DataSegment>,
    elements: Vec<TableSegment>,
    start: Option<FuncIndex>,
}

impl ModuleInfo {
    pub fn new(
        funcs: FrozenMap<FuncIndex, FuncInfo>,
        types: FrozenMap<TypeIndex, FuncType>,
        heaps: FrozenMap<HeapIndex, HeapInfo>,
        tables: FrozenMap<TableIndex, TableInfo>,
        globs: FrozenMap<GlobIndex, GlobInfo>,
        imports: FrozenMap<ImportIndex, String>,
        segments: Vec<DataSegment>,
        elements: Vec<TableSegment>,
        start: Option<FuncIndex>,
    ) -> Self {
        Self {
            exported_items: HashMap::new(),
            funcs,
            types,
            heaps,
            tables,
            globs,
            imports,
            segments,
            elements,
            start,
        }
    }

    /// Update the offset of a Wasm function.
    ///
    /// This is intended for use by the compiler, as the functions might be defined before the
    /// final layout is decided. In that case, the offsets can be set after layant rather than at
    /// declaration time.
    pub fn update_func_offset(&mut self, func_idx: FuncIndex, offset: u32) {
        match &mut self.funcs[func_idx] {
            FuncInfo::Owned {
                offset: previous_offset,
                ..
            } => *previous_offset = offset,
            FuncInfo::Imported { .. } => panic!("Tried to set offset of imported function"),
            FuncInfo::Native { .. } => panic!("Tried to set offset of a native function"),
        }
    }

    /// Marks a function as exported under the given list of names.
    pub fn export_func(&mut self, func_idx: FuncIndex, exported_names: &[String]) {
        for exported_name in exported_names {
            self.exported_items
                .insert((*exported_name).to_string(), ItemRef::Func(func_idx));
        }
    }

    /// Marks a heap as exported under the given list of names.
    pub fn export_heap(&mut self, heap_idx: HeapIndex, exported_names: &[String]) {
        for exported_name in exported_names {
            self.exported_items
                .insert((*exported_name).to_string(), ItemRef::Heap(heap_idx));
        }
    }

    /// Marks a table exported under the given list of names.
    pub fn export_table(&mut self, table_idx: TableIndex, exported_names: &[String]) {
        for exported_name in exported_names {
            self.exported_items
                .insert((*exported_name).to_string(), ItemRef::Table(table_idx));
        }
    }

    /// Marks a global exported under the given list of names.
    pub fn export_glob(&mut self, glob_idx: GlobIndex, exported_names: &[String]) {
        for exported_name in exported_names {
            self.exported_items
                .insert((*exported_name).to_string(), ItemRef::Glob(glob_idx));
        }
    }
}

/// A WebAssembly module.
pub struct WasmModule {
    exported_names: HashMap<String, ItemRef>,
    funcs: FrozenMap<FuncIndex, FuncInfo>,
    types: FrozenMap<TypeIndex, FuncType>,
    heaps: FrozenMap<HeapIndex, HeapInfo>,
    tables: FrozenMap<TableIndex, TableInfo>,
    globs: FrozenMap<GlobIndex, GlobInfo>,
    imports: FrozenMap<ImportIndex, String>,
    segments: Vec<DataSegment>,
    elements: Vec<TableSegment>,
    start: Option<FuncIndex>,
    code: Vec<u8>,
    relocs: Vec<Reloc>,
    vmctx_layout: SimpleVMContextLayout,
}

impl WasmModule {
    pub fn new(info: ModuleInfo, code: Vec<u8>, relocs: Vec<Reloc>) -> Self {
        // Compute the VMContext layout
        let nb_imported_funcs = info
            .funcs
            .values()
            .filter(|func| func.is_imported())
            .count();
        let mut funcs = Vec::with_capacity(nb_imported_funcs);
        let mut heaps = Vec::with_capacity(info.heaps.len());
        let mut tables = Vec::with_capacity(info.tables.len());
        let mut globs = Vec::with_capacity(info.globs.len());
        let mut imports = Vec::with_capacity(info.imports.len());

        for (func_idx, func) in info.funcs.iter() {
            if func.is_imported() {
                // TODO: shouldn't it be `is_exported`?
                funcs.push(func_idx);
            }
        }
        for heap_idx in info.heaps.keys() {
            heaps.push(heap_idx);
        }
        for table_idx in info.tables.keys() {
            tables.push(table_idx);
        }
        for import_idx in info.imports.keys() {
            imports.push(import_idx);
        }
        for glob_idx in info.globs.keys() {
            globs.push(glob_idx);
        }

        let vmctx_layout = SimpleVMContextLayout::new(funcs, heaps, tables, globs, imports);

        Self {
            exported_names: info.exported_items,
            funcs: info.funcs,
            types: info.types,
            heaps: info.heaps,
            tables: info.tables,
            globs: info.globs,
            imports: info.imports,
            segments: info.segments,
            elements: info.elements,
            start: info.start,
            code,
            relocs,
            vmctx_layout,
        }
    }
}

impl Module for WasmModule {
    type VMContext = SimpleVMContextLayout;

    fn start(&self) -> Option<FuncIndex> {
        self.start.clone()
    }

    fn code(&self) -> &[u8] {
        &self.code
    }

    fn heaps(&self) -> &FrozenMap<HeapIndex, HeapInfo> {
        &self.heaps
    }

    fn tables(&self) -> &FrozenMap<TableIndex, TableInfo> {
        &self.tables
    }

    fn funcs(&self) -> &FrozenMap<FuncIndex, FuncInfo> {
        &self.funcs
    }

    fn types(&self) -> &FrozenMap<crate::TypeIndex, crate::FuncType> {
        &self.types
    }

    fn globs(&self) -> &FrozenMap<GlobIndex, GlobInfo> {
        &self.globs
    }

    fn imports(&self) -> &FrozenMap<ImportIndex, String> {
        &self.imports
    }

    fn data_segments(&self) -> &[DataSegment] {
        &self.segments
    }

    fn table_segments(&self) -> &[TableSegment] {
        &self.elements
    }

    fn relocs(&self) -> &[Reloc] {
        &self.relocs
    }

    fn public_items(&self) -> &HashMap<String, ItemRef> {
        &self.exported_names
    }

    fn vmctx_layout(&self) -> &Self::VMContext {
        &self.vmctx_layout
    }
}

// ————————————————————————————— Native Module —————————————————————————————— //

static EMPTY_CODE: [u8; 0] = [];
static EMPTY_SEGMENT: [DataSegment; 0] = [];
static EMPTY_ELEMENTS: [TableSegment; 0] = [];
static EMPTY_HEAPS: FrozenMap<HeapIndex, HeapInfo> = FrozenMap::empty();
static EMPTY_GLOBS: FrozenMap<GlobIndex, GlobInfo> = FrozenMap::empty();
static EMPTY_IMPORTS: FrozenMap<ImportIndex, String> = FrozenMap::empty();
static EMPTY_RELOCS: [Reloc; 0] = [];

/// A builder for native modules.
pub struct NativeModuleBuilder {
    exported_names: HashMap<String, ItemRef>,
    funcs: PrimaryMap<FuncIndex, FuncInfo>,
    types: PrimaryMap<TypeIndex, FuncType>,
    tables: PrimaryMap<TableIndex, TableInfo>,
}

impl NativeModuleBuilder {
    /// Creates a fresh native module builder.
    pub fn new() -> Self {
        Self {
            exported_names: HashMap::new(),
            funcs: PrimaryMap::new(),
            types: PrimaryMap::new(),
            tables: PrimaryMap::new(),
        }
    }

    /// Finilizes the native module.
    pub fn build(self) -> NativeModule {
        let vmctx_layout = SimpleVMContextLayout::new(
            self.funcs.keys().collect(),
            Vec::new(),
            self.tables.keys().collect(),
            Vec::new(),
            Vec::new(),
        );
        NativeModule {
            exported_names: self.exported_names,
            funcs: FrozenMap::freeze(self.funcs),
            types: FrozenMap::freeze(self.types),
            tables: FrozenMap::freeze(self.tables),
            vmctx_layout,
        }
    }

    /// Add a native function to the module.
    ///
    /// SAFETY: there is no typecheck yet! The function might be called with unexpected number of
    /// arguments from Wasm instances!
    pub unsafe fn add_func<P, R>(mut self, name: String, func: &NativeFunc<P, R>) -> Self
    where
        P: WasmParams,
        R: WasmResults,
    {
        let ptr = FuncPtr::from_native_func(func);
        let ty = self.types.push(func.ty());
        let idx = self.funcs.push(FuncInfo::Native { ptr, ty });
        self.exported_names.insert(name, ItemRef::Func(idx));
        self
    }

    /// Add a native table to the module.
    ///
    /// TODO: add typecheck info (i.e. type of the table elements).
    pub fn add_table(mut self, name: String, table: Vec<impl WasmType<Abi = ExternRef64>>) -> Self {
        let table = table
            .iter()
            .map(|externref| externref.into_abi())
            .collect::<Vec<u64>>();
        let idx = self.tables.push(TableInfo::Native {
            ptr: table.into_boxed_slice(),
            ty: RefType::ExternRef,
        });
        self.exported_names.insert(name, ItemRef::Table(idx));
        self
    }
}

/// A module exposing native (Rust) functions and items.
pub struct NativeModule {
    exported_names: HashMap<String, ItemRef>,
    funcs: FrozenMap<FuncIndex, FuncInfo>,
    types: FrozenMap<TypeIndex, FuncType>,
    tables: FrozenMap<TableIndex, TableInfo>,
    vmctx_layout: SimpleVMContextLayout,
}

impl Module for NativeModule {
    type VMContext = SimpleVMContextLayout;

    fn start(&self) -> Option<FuncIndex> {
        None
    }

    fn code(&self) -> &[u8] {
        &EMPTY_CODE
    }

    fn heaps(&self) -> &FrozenMap<HeapIndex, HeapInfo> {
        &EMPTY_HEAPS
    }

    fn tables(&self) -> &FrozenMap<TableIndex, TableInfo> {
        &self.tables
    }

    fn funcs(&self) -> &FrozenMap<FuncIndex, FuncInfo> {
        &self.funcs
    }

    fn types(&self) -> &FrozenMap<TypeIndex, FuncType> {
        &self.types
    }

    fn globs(&self) -> &FrozenMap<GlobIndex, GlobInfo> {
        &EMPTY_GLOBS
    }

    fn imports(&self) -> &FrozenMap<ImportIndex, String> {
        &EMPTY_IMPORTS
    }

    fn data_segments(&self) -> &[DataSegment] {
        &EMPTY_SEGMENT
    }

    fn table_segments(&self) -> &[TableSegment] {
        &EMPTY_ELEMENTS
    }

    fn relocs(&self) -> &[Reloc] {
        &EMPTY_RELOCS
    }

    fn public_items(&self) -> &HashMap<String, ItemRef> {
        &self.exported_names
    }

    fn vmctx_layout(&self) -> &Self::VMContext {
        &self.vmctx_layout
    }
}
