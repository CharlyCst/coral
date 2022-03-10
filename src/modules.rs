use crate::collections::{FrozenMap, HashMap, PrimaryMap};
use crate::traits::{FuncIndex, FuncInfo, HeapIndex, HeapInfo, HeapKind, Reloc};
use crate::traits::{ItemRef, Module};

// ————————————————————————————————— Module ————————————————————————————————— //

pub struct ModuleInfo {
    exported_items: HashMap<String, ItemRef>,
    funcs: PrimaryMap<FuncIndex, FuncInfo>,
    heaps: PrimaryMap<HeapIndex, HeapInfo>,
}

impl ModuleInfo {
    pub fn new() -> Self {
        Self {
            exported_items: HashMap::new(),
            funcs: PrimaryMap::new(),
            heaps: PrimaryMap::new(),
        }
    }

    pub fn register_func(&mut self, exported_names: &Vec<String>, offset: u32) {
        let func_info = FuncInfo::Owned { offset };
        let idx = self.funcs.push(func_info);
        let item = ItemRef::Func(idx);

        // Export the function, if required
        for exported_name in exported_names {
            self.exported_items.insert(exported_name.to_owned(), item);
        }
    }

    pub fn register_imported_func(
        &mut self,
        exported_names: &Vec<String>,
        module: String,
        name: String,
    ) {
        let func_info = FuncInfo::Imported { module, name };
        let idx = self.funcs.push(func_info);
        let item = ItemRef::Func(idx);

        // Export the function, if required
        for exported_name in exported_names {
            self.exported_items.insert(exported_name.to_owned(), item);
        }
    }

    pub fn register_heap(&mut self, min_size: u32, max_size: Option<u32>) {
        let kind = match max_size {
            Some(max_size) => HeapKind::Static { max_size },
            None => HeapKind::Dynamic,
        };
        self.heaps.push(HeapInfo {
            min_size,
            max_size,
            kind,
        });
    }
}

pub struct SimpleModule {
    exported_names: HashMap<String, ItemRef>,
    funcs: FrozenMap<FuncIndex, FuncInfo>,
    heaps: FrozenMap<HeapIndex, HeapInfo>,
    code: Vec<u8>,
    relocs: Vec<Reloc>,
    vmctx_layout: Vec<ItemRef>,
}

impl SimpleModule {
    pub fn new(info: ModuleInfo, code: Vec<u8>, relocs: Vec<Reloc>) -> Self {
        // Compute the VMContext layout
        let heaps = &info.heaps;
        let mut vmctx_layout = Vec::with_capacity(heaps.len());
        for heap_idx in heaps.keys() {
            vmctx_layout.push(ItemRef::Heap(heap_idx));
        }

        Self {
            exported_names: info.exported_items,
            funcs: FrozenMap::freeze(info.funcs),
            heaps: FrozenMap::freeze(info.heaps),
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


