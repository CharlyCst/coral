use crate::traits::{
    FuncIndex, FuncInfo, GlobIndex, GlobInfo, HeapIndex, HeapInfo, ImportIndex, Reloc,
};
use crate::traits::{ItemRef, Module, VMContextLayout};
use ocean_collections::{FrozenMap, HashMap};

// ————————————————————————————————— Module ————————————————————————————————— //

pub struct ModuleInfo {
    exported_items: HashMap<String, ItemRef>,
    funcs: FrozenMap<FuncIndex, FuncInfo>,
    heaps: FrozenMap<HeapIndex, HeapInfo>,
    globs: FrozenMap<GlobIndex, GlobInfo>,
    imports: FrozenMap<ImportIndex, String>,
}

impl ModuleInfo {
    pub fn new(
        funcs: FrozenMap<FuncIndex, FuncInfo>,
        heaps: FrozenMap<HeapIndex, HeapInfo>,
        globs: FrozenMap<GlobIndex, GlobInfo>,
        imports: FrozenMap<ImportIndex, String>,
    ) -> Self {
        Self {
            exported_items: HashMap::new(),
            funcs,
            heaps,
            globs,
            imports,
        }
    }

    pub fn set_func_offset(&mut self, func_idx: FuncIndex, offset: u32) {
        match &mut self.funcs[func_idx] {
            FuncInfo::Owned {
                offset: previous_offset,
                ..
            } => *previous_offset = offset,
            &mut FuncInfo::Imported { .. } => panic!("Tried to set offset of imported function"),
        }
    }

    /// Mark a function as exported under the given list of names.
    pub fn export_func(&mut self, func_idx: FuncIndex, exported_names: &[String]) {
        for exported_name in exported_names {
            self.exported_items
                .insert((*exported_name).to_owned(), ItemRef::Func(func_idx));
        }
    }

    pub fn export_heap(&mut self, heap_idx: HeapIndex, exported_names: &[String]) {
        for exported_name in exported_names {
            self.exported_items
                .insert((*exported_name).to_owned(), ItemRef::Heap(heap_idx));
        }
    }

    pub fn export_glob(&mut self, glob_idx: GlobIndex, exported_names: &[String]) {
        for exported_name in exported_names {
            self.exported_items
                .insert((*exported_name).to_owned(), ItemRef::Glob(glob_idx));
        }
    }
}

#[derive(Clone)]
pub struct SimpleVMContextLayout {
    funcs: Vec<FuncIndex>,
    heaps: Vec<HeapIndex>,
    globs: Vec<GlobIndex>,
    imports: Vec<ImportIndex>,
}

impl SimpleVMContextLayout {
    pub fn new(
        funcs: Vec<FuncIndex>,
        heaps: Vec<HeapIndex>,
        globs: Vec<GlobIndex>,
        imports: Vec<ImportIndex>,
    ) -> Self {
        Self {
            funcs,
            heaps,
            globs,
            imports,
        }
    }
}

impl VMContextLayout for SimpleVMContextLayout {
    fn heaps(&self) -> &[HeapIndex] {
        &self.heaps
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

pub struct SimpleModule {
    exported_names: HashMap<String, ItemRef>,
    funcs: FrozenMap<FuncIndex, FuncInfo>,
    heaps: FrozenMap<HeapIndex, HeapInfo>,
    globs: FrozenMap<GlobIndex, GlobInfo>,
    imports: FrozenMap<ImportIndex, String>,
    code: Vec<u8>,
    relocs: Vec<Reloc>,
    vmctx_layout: SimpleVMContextLayout,
}

impl SimpleModule {
    pub fn new(info: ModuleInfo, code: Vec<u8>, relocs: Vec<Reloc>) -> Self {
        // Compute the VMContext layout
        let nb_imported_funcs = info
            .funcs
            .values()
            .filter(|func| func.is_imported())
            .count();
        let mut funcs = Vec::with_capacity(nb_imported_funcs);
        let mut heaps = Vec::with_capacity(info.heaps.len());
        let mut globs = Vec::with_capacity(info.globs.len());
        let mut imports = Vec::with_capacity(info.imports.len());

        for (func_idx, func) in info.funcs.iter() {
            if func.is_imported() {
                funcs.push(func_idx);
            }
        }
        for heap_idx in info.heaps.keys() {
            heaps.push(heap_idx);
        }
        for import_idx in info.imports.keys() {
            imports.push(import_idx);
        }
        for glob_idx in info.globs.keys() {
            globs.push(glob_idx);
        }

        let vmctx_layout = SimpleVMContextLayout::new(funcs, heaps, globs, imports);

        Self {
            exported_names: info.exported_items,
            funcs: info.funcs,
            heaps: info.heaps,
            globs: info.globs,
            imports: info.imports,
            code,
            relocs,
            vmctx_layout,
        }
    }
}

impl Module for SimpleModule {
    type VMContext = SimpleVMContextLayout;

    fn code(&self) -> &[u8] {
        &self.code
    }

    fn heaps(&self) -> &FrozenMap<HeapIndex, HeapInfo> {
        &self.heaps
    }

    fn funcs(&self) -> &FrozenMap<FuncIndex, FuncInfo> {
        &self.funcs
    }

    fn globs(&self) -> &FrozenMap<GlobIndex, GlobInfo> {
        &self.globs
    }

    fn imports(&self) -> &FrozenMap<ImportIndex, String> {
        &self.imports
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
