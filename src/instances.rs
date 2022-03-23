#![allow(unused)]

use crate::collections::{EntityRef, FrozenMap, HashMap, PrimaryMap};
use crate::traits::{Allocator, GlobIndex, Module, ModuleError, ModuleResult, VMContextLayout};
use crate::traits::{
    FuncIndex, FuncInfo, GlobInit, HeapIndex, HeapKind, ImportIndex, ItemRef, RelocKind,
};
use crate::vmctx::VMContext;

enum Item<'a, Alloc: Allocator> {
    Func(&'a Func),
    Heap(&'a Heap<Alloc>),
}

struct Heap<Alloc: Allocator> {
    memory: Box<[u8], Alloc::HeapAllocator>,
}

impl<Alloc: Allocator> Heap<Alloc> {
    /// Create a new heap with the given capacity, in number of pages.
    pub fn with_capacity(capacity: u32, kind: HeapKind, alloc: &Alloc) -> Self {
        let mut memory = alloc.alloc_heap(capacity, kind);
        for byte in memory.iter_mut() {
            *byte = 0;
        }
        Self { memory }
    }

    /// Get the pointer to the heap data.
    pub fn ptr(&mut self) -> *mut u8 {
        self.memory.as_mut_ptr()
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

pub struct Instance<Alloc: Allocator> {
    /// A map of all exported symbols.
    items: HashMap<String, ItemRef>,

    /// The VM Context, contains pointers to various structures, such as heaps and tables.
    ///
    /// For now, only 8 bytes pointers are handled.
    vmctx: VMContext,

    /// The heaps of the instance.
    heaps: FrozenMap<HeapIndex, Heap<Alloc>>,

    /// The functions of the instance.
    funcs: FrozenMap<FuncIndex, Func>,

    /// The global variables of the instance.
    globs: FrozenMap<GlobIndex, Glob>,

    /// The imported instances.
    imports: FrozenMap<ImportIndex, Instance<Alloc>>,

    /// The memory region containing the code
    code: Box<[u8], Alloc::CodeAllocator>,
}

impl<Alloc: Allocator> Instance<Alloc> {
    pub fn instantiate<Mod: Module>(
        module: &Mod,
        import_from: Vec<(&str, Instance<Alloc>)>,
        alloc: &Alloc,
    ) -> ModuleResult<Self>
    where
        Alloc: Allocator,
    {
        let mut import_from = import_from
            .into_iter()
            .map(|x| Some(x))
            .collect::<Vec<Option<(&str, Instance<Alloc>)>>>();
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
            crate::traits::GlobInfo::Owned { init } => Ok(Glob::Owned { init: *init }),
            crate::traits::GlobInfo::Imported { module, name } => {
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

        let items = module.public_items().to_owned();

        // Allocate heaps
        let heaps = module
            .heaps()
            .map(|heap_info| Heap::with_capacity(heap_info.min_size, heap_info.kind, alloc));

        // Allocate code
        let mod_code = module.code();
        let mut code = alloc.alloc_code(mod_code.len() as u32);
        code.copy_from_slice(mod_code);

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
        alloc.set_executable(&instance.code);

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
                ItemRef::Func(func) => self.get_func_addr(func) as i64,
                // Only functions are supported by relocations
                _ => return Err(ModuleError::FailedToInstantiate),
            };
            let value = base + reloc.addend;

            let offset = reloc.offset as usize;
            match reloc.kind {
                RelocKind::Abs4 => todo!(),
                RelocKind::Abs8 => {
                    self.code[offset..][..8].copy_from_slice(&value.to_le_bytes());
                }
                RelocKind::X86PCRel4 => todo!(),
                RelocKind::X86CallPCRel4 => {
                    let pc = self.code.as_ptr().wrapping_add(reloc.offset as usize) as i64;
                    let pc_relative = (value - pc) as i32;
                    self.code[offset..][..4].copy_from_slice(&pc_relative.to_le_bytes());
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
    /// Imported functions are resolved though recursive lookups.
    fn get_func_addr(&self, func: FuncIndex) -> *const u8 {
        match &self.funcs[func] {
            Func::Owned { offset } => self.code.as_ptr().wrapping_add(*offset as usize),
            Func::Imported { from, index } => {
                let instance = &self.imports[*from];
                instance.get_func_addr(*index)
            }
        }
    }

    /// Return the initial value of a global.
    /// TODO: the current structure of the VMContext is not rich enough to express global
    /// variables, there are different types that we might wants to return, including types bigger
    /// than 8 bytes. Plus pointers are too small on 32 bits architectures...
    fn get_glob_init_value(&self, glob: GlobIndex) -> *const u8 {
        match &self.globs[glob] {
            Glob::Owned { init } => match init {
                // TODO: how to perform the conversion? We need a better VMContext...
                GlobInit::I32(x) => (*x as i64) as *const u8,
                GlobInit::I64(x) => *x as *const u8,
                GlobInit::F32(x) => unsafe { core::mem::transmute(*x as u64) },
                GlobInit::F64(x) => unsafe { core::mem::transmute(*x) },
            },
            Glob::Imported { from, index } => {
                let instance = &self.imports[*from];
                instance.get_glob_ptr(*index)
            }
        }
    }

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
        for (idx, heap) in self.heaps.iter_mut() {
            let ptr = heap.ptr();
            self.vmctx.set_heap(ptr, idx);
        }
        for idx in self.funcs.keys() {
            let ptr = self.get_func_addr(idx);
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

    fn get_item(&self, item: ItemRef) -> Item<'_, Alloc> {
        match item {
            ItemRef::Func(idx) => Item::Func(&self.funcs[idx]),
            ItemRef::Heap(idx) => Item::Heap(&self.heaps[idx]),
            ItemRef::Glob(idx) => todo!(),
            ItemRef::Import(idx) => todo!(),
        }
    }
}
