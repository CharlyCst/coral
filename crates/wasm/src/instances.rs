#![allow(unused)]

use crate::alloc::boxed::Box;
use crate::alloc::string::String;
use crate::alloc::vec::Vec;

use crate::traits::{
    FuncIndex, FuncInfo, GlobInfo, GlobInit, HeapIndex, HeapInfo, HeapKind, ImportIndex, ItemRef,
    RelocKind,
};
use crate::traits::{
    GlobIndex, MemoryAeaAllocator, MemoryArea, Module, ModuleError, ModuleResult, VMContextLayout,
};
use crate::vmctx::VMContext;
use alloc::borrow::ToOwned;
use ocean_collections::{EntityRef, FrozenMap, HashMap, PrimaryMap};

enum Item<'a, Area: MemoryArea> {
    Func(&'a Func),
    Heap(&'a Heap<Area>),
}

enum Heap<Area: MemoryArea> {
    Owned { memory: Area },
    Imported { from: ImportIndex, index: HeapIndex },
}

impl<Area: MemoryArea> Heap<Area> {
    /// Create a new heap with the given capacity, in number of pages.
    pub fn with_capacity(capacity: u32, kind: HeapKind, mut area: Area) -> Self {
        area.extend_to(capacity as usize);
        // Zero-out the area
        for byte in area.as_bytes_mut().iter_mut() {
            *byte = 0;
        }
        Self::Owned { memory: area }
    }
}

enum Func {
    Owned { offset: u32 },
    Imported { from: ImportIndex, index: FuncIndex },
}

enum Glob {
    Owned { init: GlobInit },
    Imported { from: ImportIndex, index: GlobIndex },
}

pub struct Instance<Area: MemoryArea> {
    /// A map of all exported symbols.
    items: HashMap<String, ItemRef>,

    /// The VM Context, contains pointers to various structures, such as heaps and tables.
    ///
    /// For now, only 8 bytes pointers are handled.
    vmctx: VMContext,

    /// The heaps of the instance.
    heaps: FrozenMap<HeapIndex, Heap<Area>>,

    /// The functions of the instance.
    funcs: FrozenMap<FuncIndex, Func>,

    /// The global variables of the instance.
    globs: FrozenMap<GlobIndex, Glob>,

    /// The imported instances.
    imports: FrozenMap<ImportIndex, Instance<Area>>,

    /// The memory region containing the code
    code: Area,
}

impl<Area: MemoryArea> Instance<Area> {
    pub fn instantiate<Mod: Module>(
        module: &Mod,
        import_from: Vec<(&str, Instance<Area>)>,
        alloc: &impl MemoryAeaAllocator<Area=Area>,
    ) -> ModuleResult<Self> {
        let mut import_from = import_from
            .into_iter()
            .map(|x| Some(x))
            .collect::<Vec<Option<(&str, Instance<Area>)>>>();
        let imports = module.imports().try_map(|module| {
            // Pick the first matching module
            for item in import_from.iter_mut() {
                if let Some((item_name, instance)) = item {
                    if item_name == module {
                        let (_, instance) = item.take().unwrap();
                        return Ok(instance);
                    }
                }
            }
            Err(ModuleError::FailedToInstantiate)
        })?;

        let funcs = module.funcs().try_map(|func_info| match func_info {
            FuncInfo::Owned { offset } => Ok(Func::Owned { offset: *offset }),
            FuncInfo::Imported { module, name } => {
                // Look for the corresponding module
                let instance = &imports[*module];
                let func_ref = instance
                    .items
                    .get(name)
                    .ok_or(ModuleError::FailedToInstantiate)?
                    .as_func()
                    .ok_or(ModuleError::FailedToInstantiate)?;

                // TODO: typecheck the function here
                let _func = &instance.funcs[func_ref];

                Ok(Func::Imported {
                    from: *module,
                    index: func_ref,
                })
            }
        })?;

        let globs = module.globs().try_map(|glob_info| match glob_info {
            GlobInfo::Owned { init } => Ok(Glob::Owned { init: *init }),
            GlobInfo::Imported { module, name } => {
                // Look for the corresponding module
                let instance = &imports[*module];
                let glob_ref = instance
                    .items
                    .get(name)
                    .ok_or(ModuleError::FailedToInstantiate)?
                    .as_glob()
                    .ok_or(ModuleError::FailedToInstantiate)?;

                // TODO: typecheck glob here
                let _glob = &instance.globs[glob_ref];

                Ok(Glob::Imported {
                    from: *module,
                    index: glob_ref,
                })
            }
        })?;

        let items = module.public_items().clone();

        // Allocate heaps
        let heaps = module.heaps().try_map(|heap_info| match heap_info {
            HeapInfo::Owned { min_size, kind } => {
                let area = alloc
                    .with_capacity(*min_size as usize)
                    .map_err(|_| ModuleError::FailedToInstantiate)?;
                Ok(Heap::with_capacity(*min_size, *kind, area))
            }
            HeapInfo::Imported { module, name } => {
                // Look for the corresponding module
                let instance = &imports[*module];
                let heap_ref = instance
                    .items
                    .get(name)
                    .ok_or(ModuleError::FailedToInstantiate)?
                    .as_heap()
                    .ok_or(ModuleError::FailedToInstantiate)?;

                Ok(Heap::Imported {
                    from: *module,
                    index: heap_ref,
                })
            }
        })?;

        // Allocate code
        let mod_code = module.code();
        let mut code = alloc
            .with_capacity(mod_code.len())
            .map_err(|_| ModuleError::FailedToInstantiate)?;
        code.as_bytes_mut()[..mod_code.len()].copy_from_slice(mod_code);

        // Create instance
        let mut instance = Self {
            vmctx: VMContext::empty(module.vmctx_layout()),
            imports,
            items,
            heaps,
            globs,
            funcs,
            code,
        };

        // Relocate & set code execute-only
        instance.relocate(module)?;
        instance.code.set_executable();

        // Set the VMContext to its expected initial values
        instance.init_vmctx();

        Ok(instance)
    }

    /// Returns the address of a function exported by the instance.
    pub fn get_func_addr_from_name<'a, 'b>(&'a self, name: &'b str) -> Option<*const u8> {
        let name = self.items.get(name)?;
        let func = self.get_func(*name)?;

        match func {
            Func::Owned { offset } => {
                let addr = self.code.as_ptr();

                // SAFETY: We rely on the function offset being correct here, in which case the offset is
                // less or equal to `code.len()` and points to the start of the intended function.
                unsafe { Some(addr.offset(*offset as isize)) }
            }
            Func::Imported { from, index: func } => todo!(),
        }
    }

    pub fn get_vmctx_ptr(&self) -> *const u8 {
        self.vmctx.as_ptr()
    }

    fn relocate(&mut self, module: &impl Module) -> ModuleResult<()> {
        for reloc in module.relocs() {
            let base = match reloc.item {
                ItemRef::Func(func) => self.get_func_ptr(func) as i64,
                // Only functions are supported by relocations
                _ => return Err(ModuleError::FailedToInstantiate),
            };
            let value = base + reloc.addend;

            let offset = reloc.offset as usize;
            let code = self.code.as_bytes_mut();
            match reloc.kind {
                RelocKind::Abs4 => todo!(),
                RelocKind::Abs8 => {
                    code[offset..][..8].copy_from_slice(&value.to_le_bytes());
                }
                RelocKind::X86PCRel4 => todo!(),
                RelocKind::X86CallPCRel4 => {
                    let pc = code.as_ptr().wrapping_add(reloc.offset as usize) as i64;
                    let pc_relative = (value - pc) as i32;
                    code[offset..][..4].copy_from_slice(&pc_relative.to_le_bytes());
                }
                RelocKind::X86CallPLTRel4 => todo!(),
                RelocKind::X86GOTPCRel4 => todo!(),
                RelocKind::Arm32Call => todo!(),
                RelocKind::Arm64Call => todo!(),
                RelocKind::S390xPCRel32Dbl => todo!(),
                RelocKind::ElfX86_64TlsGd => todo!(),
                RelocKind::MachOX86_64Tlv => todo!(),
                RelocKind::Aarch64TlsGdAdrPage21 => todo!(),
                RelocKind::Aarch64TlsGdAddLo12Nc => todo!(),
            }
        }

        Ok(())
    }

    /// Return the address of a function.
    /// Imported functions are resolved through recursive lookups.
    fn get_func_ptr(&self, func: FuncIndex) -> *const u8 {
        match &self.funcs[func] {
            Func::Owned { offset } => self.code.as_ptr().wrapping_add(*offset as usize),
            Func::Imported { from, index } => {
                let instance = &self.imports[*from];
                instance.get_func_ptr(*index)
            }
        }
    }

    /// Return the address of a heap.
    /// Imported heaps are resolved through recursive lookups.
    fn get_heap_ptr(&self, heap: HeapIndex) -> *const u8 {
        match &self.heaps[heap] {
            Heap::Owned { memory } => memory.as_ptr(),
            Heap::Imported { from, index } => {
                let instance = &self.imports[*from];
                instance.get_heap_ptr(*index)
            }
        }
    }

    /// Return the address of a global.
    /// Imported globals are resolved through recursive lookups.
    fn get_glob_ptr(&self, glob: GlobIndex) -> *const u8 {
        match &self.globs[glob] {
            Glob::Owned { .. } => self.vmctx.get_global_ptr(glob),
            Glob::Imported { from, index } => {
                let instance = &self.imports[*from];
                instance.get_glob_ptr(*index)
            }
        }
    }

    /// Initialize the VMContext struct.
    /// This function **must** be called before runing any code within the instance, otherwise the
    /// execution leads to undefined behavior.
    fn init_vmctx(&mut self) {
        for idx in self.heaps.keys() {
            let ptr = self.get_heap_ptr(idx);
            self.vmctx.set_heap(ptr, idx);
        }
        for idx in self.funcs.keys() {
            let ptr = self.get_func_ptr(idx);
            self.vmctx.set_func(ptr, idx);
        }
        for (idx, import) in self.imports.iter_mut() {
            let ptr = import.vmctx.as_ptr();
            self.vmctx.set_import(ptr, idx);
        }
        for (idx, glob) in self.globs.iter() {
            match glob {
                Glob::Owned { init } => self.vmctx.set_glob_value(*init, idx),
                Glob::Imported { .. } => self.vmctx.set_glob_ptr(self.get_glob_ptr(idx), idx),
            }
        }
    }

    fn get_func(&self, item: ItemRef) -> Option<&Func> {
        match self.get_item(item) {
            Item::Func(func) => Some(func),
            _ => None,
        }
    }

    fn get_item(&self, item: ItemRef) -> Item<'_, Area> {
        match item {
            ItemRef::Func(idx) => Item::Func(&self.funcs[idx]),
            ItemRef::Heap(idx) => Item::Heap(&self.heaps[idx]),
            ItemRef::Glob(idx) => todo!(),
            ItemRef::Import(idx) => todo!(),
        }
    }
}
