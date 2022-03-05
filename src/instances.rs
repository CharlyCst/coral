#![allow(unused)]

use crate::collections::{FrozenMap, HashMap, PrimaryMap};
use crate::traits::{Allocator, Module, ModuleError, ModuleResult};
use crate::traits::{FuncIndex, FuncInfo, HeapIndex, HeapKind, ImportIndex, ItemRef, RelocKind};

type VMContext = Vec<*const u8>;

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

pub struct Instance<Alloc: Allocator> {
    /// A map of all exported symbols.
    items: HashMap<String, ItemRef>,

    /// The list of items in the VMContext. Used to dynamically update the VMContext.
    vmctx_items: Vec<ItemRef>,

    /// The VM Context, contains pointers to various structures, such as heaps and tables.
    ///
    /// For now, only 8 bytes pointers are handled.
    vmctx: VMContext,

    /// The heaps of the instance.
    heaps: FrozenMap<HeapIndex, Heap<Alloc>>,

    /// The functions of the instance.
    funcs: FrozenMap<FuncIndex, Func>,

    /// The imported instances.
    imports: FrozenMap<ImportIndex, Instance<Alloc>>,

    /// The memory region containing the code
    code: Box<[u8], Alloc::CodeAllocator>,
}

impl<Alloc: Allocator> Instance<Alloc> {
    pub fn instantiate(
        module: &impl Module,
        import_from: Vec<(&str, Instance<Alloc>)>,
        alloc: &Alloc,
    ) -> ModuleResult<Self>
    where
        Alloc: Allocator,
    {
        let mut imports = PrimaryMap::with_capacity(import_from.len());
        let imports_refs = import_from
            .into_iter()
            .map(|(name, instance)| {
                let idx: ImportIndex = imports.push(instance);
                (name, idx)
            })
            .collect::<Vec<(&str, ImportIndex)>>();
        let imports = FrozenMap::freeze(imports);

        let funcs = module.funcs().try_map(|func_info| match func_info {
            FuncInfo::Owned { offset } => Ok(Func::Owned { offset: *offset }),
            FuncInfo::Imported { module, name } => {
                // Look for the corresponding module
                // NOTE: maybe we want a smarter lookup strategy?
                let (_, import_idx) = imports_refs
                    .iter()
                    .find(|(m, idx)| m == module)
                    .ok_or(ModuleError::FailedToInstantiate)?;
                let instance = &imports[*import_idx];
                let func_ref = instance
                    .items
                    .get(name)
                    .ok_or(ModuleError::FailedToInstantiate)?
                    .as_func()
                    .ok_or(ModuleError::FailedToInstantiate)?;

                // TODO: typecheck the function here
                let _func = &instance.funcs[func_ref];

                Ok(Func::Imported {
                    from: *import_idx,
                    index: func_ref,
                })
            }
        })?;

        let items = module.public_items().to_owned();
        let vmctx_items = module.vmctx_items().to_owned();

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
            vmctx: vec![core::ptr::NonNull::dangling().as_ptr(); vmctx_items.len()],
            vmctx_items,
            imports,
            items,
            heaps,
            funcs,
            code,
        };

        // Relocate & set code execute-only
        instance.relocate(module)?;
        alloc.set_executable(&instance.code);

        // Set the VMContext to its expected initial values
        instance.update_vmctx();

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

    pub fn get_vmctx(&self) -> &VMContext {
        &self.vmctx
    }

    fn relocate(&mut self, module: &impl Module) -> ModuleResult<()> {
        for reloc in module.relocs() {
            let value = match reloc.item {
                ItemRef::Func(func) => self.get_func_addr(func),
                // Only functions are supported by relocations
                _ => return Err(ModuleError::FailedToInstantiate),
            };
            let value = value + reloc.addend;

            let offset = reloc.offset as usize;
            match reloc.kind {
                RelocKind::Abs4 => todo!(),
                RelocKind::Abs8 => {
                    self.code[offset..][..8].copy_from_slice(&value.to_le_bytes());
                }
                RelocKind::X86PCRel4 => todo!(),
                RelocKind::X86CallPCRel4 => todo!(),
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
    fn get_func_addr(&self, func: FuncIndex) -> i64 {
        match &self.funcs[func] {
            Func::Owned { offset } => self.code.as_ptr() as i64 + *offset as i64,
            Func::Imported { from, index } => {
                let instance = &self.imports[*from];
                instance.get_func_addr(*index)
            }
        }
    }

    // Create the VMContext, a structure containing pointers to the VM's items and that can be
    // queried during code execution.
    fn update_vmctx(&mut self) {
        // WARNING: `vmctx` and `vmctx_items` must have the exact same size here!
        assert_eq!(self.vmctx_items.len(), self.vmctx.len());
        for (idx, name) in self.vmctx_items.iter().enumerate() {
            let addr = match self.get_item(*name) {
                Item::Func(func) => panic!("VMContext does not support function addresses"),
                Item::Heap(heap) => heap.memory.as_ptr(),
            };
            self.vmctx[idx] = addr;
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
        }
    }
}
