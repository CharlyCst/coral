#![allow(unused)]

use crate::collections::{FrozenMap, HashMap};
use crate::traits::{Allocator, FuncIndex, Module, ModuleError, ModuleResult};
use crate::traits::{HeapIndex, HeapKind, Name, RelocKind};

type VMContext = Vec<*const u8>;

enum Symbol<'a, Alloc: Allocator> {
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

struct Func {
    offset: u32,
}

pub struct Instance<Alloc: Allocator> {
    /// A map of all exported symbols.
    symbols: HashMap<String, Name>,

    /// The list of items in the VMContext. Used to dynamically update the VMContext.
    vmctx_items: Vec<Name>,

    /// The VM Context, contains pointers to various structures, such as heaps and tables.
    ///
    /// For now, only 8 bytes pointers are handled.
    vmctx: VMContext,

    /// The heaps of the instance.
    heaps: FrozenMap<HeapIndex, Heap<Alloc>>,

    /// The functions of the instance.
    funcs: FrozenMap<FuncIndex, Func>,

    /// The memory region containing the code
    code: Box<[u8], Alloc::CodeAllocator>,
}

impl<Alloc: Allocator> Instance<Alloc> {
    pub fn instantiate(module: impl Module, alloc: &Alloc) -> ModuleResult<Self>
    where
        Alloc: Allocator,
    {
        let funcs = module.funcs().map(|func_info| Func {
            offset: func_info.offset,
        });
        let symbols = module.public_symbols().clone();
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
            symbols,
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

    /// Returns the address of a function exported bu the instance.
    pub fn get_func_addr<'a, 'b>(&'a self, name: &'b str) -> Option<*const u8> {
        let name = self.symbols.get(name)?;
        let func = self.get_func_from_name(*name)?;
        let addr = self.code.as_ptr();

        // SAFETY: We rely on the function offset being correct here, in which case the offset is
        // less or equal to `code.len()` and points to the start of the intended function.
        unsafe { Some(addr.offset(func.offset as isize)) }
    }

    pub fn get_vmctx(&self) -> &VMContext {
        &self.vmctx
    }

    fn relocate(&mut self, module: impl Module) -> ModuleResult<()> {
        for reloc in module.relocs() {
            let value = if let Some(func) = self.get_func_from_name(reloc.name) {
                self.code.as_ptr() as i64 + func.offset as i64 + reloc.addend
            } else {
                return Err(ModuleError::FailedToInstantiate);
            };
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

    // Create the VMContext, a structure containing pointers to the VM's items and that can be
    // queried during code execution.
    fn update_vmctx(&mut self) {
        // WARNING: `vmctx` and `vmctx_items` must have the exact same size here!
        assert_eq!(self.vmctx_items.len(), self.vmctx.len());
        for (idx, name) in self.vmctx_items.iter().enumerate() {
            let addr = match self.get_symbol(*name) {
                Symbol::Func(func) => unsafe { self.code.as_ptr().offset(func.offset as isize) },
                Symbol::Heap(heap) => heap.memory.as_ptr(),
            };
            self.vmctx[idx] = addr;
        }
    }

    fn get_func_from_name(&self, name: Name) -> Option<&Func> {
        match self.get_symbol(name) {
            Symbol::Func(func) => Some(func),
            _ => None,
        }
    }

    fn get_symbol(&self, name: Name) -> Symbol<'_, Alloc> {
        match name {
            Name::Imported { from, item } => todo!(), // Imports are not supported yet
            Name::Owned(item) => match item {
                crate::traits::ModuleItem::Func(idx) => Symbol::Func(&self.funcs[idx]),
                crate::traits::ModuleItem::Heap(idx) => Symbol::Heap(&self.heaps[idx]),
            },
        }
    }
}
