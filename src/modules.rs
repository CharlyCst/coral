use crate::collections::{FrozenMap, HashMap};
use crate::traits::{FuncIndex, FuncInfo, HeapIndex, HeapInfo, ImportIndex, Reloc};
use crate::traits::{ItemRef, Module};

// ————————————————————————————————— Module ————————————————————————————————— //

pub struct ModuleInfo {
    exported_items: HashMap<String, ItemRef>,
    funcs: FrozenMap<FuncIndex, FuncInfo>,
    heaps: FrozenMap<HeapIndex, HeapInfo>,
    modules: FrozenMap<ImportIndex, String>,
}

impl ModuleInfo {
    pub fn new(
        funcs: FrozenMap<FuncIndex, FuncInfo>,
        heaps: FrozenMap<HeapIndex, HeapInfo>,
        modules: FrozenMap<ImportIndex, String>,
    ) -> Self {
        Self {
            exported_items: HashMap::new(),
            funcs,
            heaps,
            modules,
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
}

pub struct SimpleModule {
    exported_names: HashMap<String, ItemRef>,
    funcs: FrozenMap<FuncIndex, FuncInfo>,
    heaps: FrozenMap<HeapIndex, HeapInfo>,
    modules: FrozenMap<ImportIndex, String>,
    code: Vec<u8>,
    relocs: Vec<Reloc>,
    vmctx_layout: Vec<ItemRef>,
}

impl SimpleModule {
    pub fn new(info: ModuleInfo, code: Vec<u8>, relocs: Vec<Reloc>) -> Self {
        // Compute the VMContext layout
        let heaps = info.heaps;
        let funcs = info.funcs;
        let modules = info.modules;
        let nb_imported_funcs = funcs.values().filter(|func| func.is_imported()).count();
        let mut vmctx_layout = Vec::with_capacity(heaps.len() + nb_imported_funcs + modules.len());

        for heap_idx in heaps.keys() {
            vmctx_layout.push(ItemRef::Heap(heap_idx));
        }
        for (func_idx, func) in funcs.iter() {
            if func.is_imported() {
                vmctx_layout.push(ItemRef::Func(func_idx));
            }
        }
        for import_idx in modules.keys() {
            vmctx_layout.push(ItemRef::Import(import_idx));
        }

        Self {
            exported_names: info.exported_items,
            funcs,
            heaps,
            modules,
            code,
            relocs,
            vmctx_layout,
        }
    }
}

impl Module for SimpleModule {
    fn code(&self) -> &[u8] {
        &self.code
    }

    fn heaps(&self) -> &FrozenMap<HeapIndex, HeapInfo> {
        &self.heaps
    }

    fn funcs(&self) -> &FrozenMap<FuncIndex, FuncInfo> {
        &self.funcs
    }

    fn imports(&self) -> &FrozenMap<ImportIndex, String> {
        &self.modules
    }

    fn relocs(&self) -> &[Reloc] {
        &self.relocs
    }

    fn vmctx_items(&self) -> &[ItemRef] {
        &self.vmctx_layout
    }

    fn public_items(&self) -> &HashMap<String, ItemRef> {
        &self.exported_names
    }
}
