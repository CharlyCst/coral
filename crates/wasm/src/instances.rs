#![allow(unused)]

use crate::alloc::borrow::ToOwned;
use crate::alloc::boxed::Box;
use crate::alloc::string::String;
use crate::alloc::vec::Vec;

use crate::traits::{
    ExclusiveMemoryArea, FuncIndex, FuncInfo, GlobInfo, GlobInit, HeapIndex, HeapInfo, HeapKind,
    ImportIndex, ItemRef, RawFuncPtr, RelocKind, TableIndex,
};
use crate::traits::{
    GlobIndex, MemoryAeaAllocator, MemoryArea, Module, ModuleError, ModuleResult, VMContextLayout,
};
use crate::vmctx::VMContext;
use collections::{EntityRef, FrozenMap, HashMap, PrimaryMap};

enum Item<'a, Area: MemoryArea> {
    Func(&'a Func),
    Heap(&'a Heap<Area>),
    Table(&'a Table),
}

enum Heap<Area> {
    Owned { memory: Area },
    Imported { from: ImportIndex, index: HeapIndex },
}

impl<Area: MemoryArea> Heap<Area> {
    /// Initializes a new heap with the given capacity, in number of pages.
    pub fn with_capacity<ExclArea>(
        capacity: u32,
        kind: HeapKind,
        mut area: ExclArea,
    ) -> Result<Self, ()>
    where
        ExclArea: ExclusiveMemoryArea<Shared = Area>,
    {
        area.extend_to(capacity as usize)?;
        // Zero-out the area
        for byte in area.as_bytes_mut().iter_mut() {
            *byte = 0;
        }
        Ok(Self::Owned {
            memory: area.into_shared(),
        })
    }
}

enum Table {
    // Note: for now we use boxed slices, so that we don't have to handle table relocation (but we
    // only support fixed size tables then...)
    Owned(Box<[u64]>),
    Imported {
        from: ImportIndex,
        index: TableIndex,
    },
}

enum Func {
    Owned { offset: u32 },
    Imported { from: ImportIndex, index: FuncIndex },
    Native { ptr: RawFuncPtr },
}

enum Glob {
    Owned { init: GlobInit },
    Imported { from: ImportIndex, index: GlobIndex },
}

pub struct Instance<Area> {
    /// A map of all exported symbols.
    items: HashMap<String, ItemRef>,

    /// The VM Context, contains pointers to various structures, such as heaps and tables.
    ///
    /// For now, only 8 bytes pointers are handled.
    vmctx: VMContext,

    /// The heaps of the instance.
    heaps: FrozenMap<HeapIndex, Heap<Area>>,

    /// The tables of the instance.
    tables: FrozenMap<TableIndex, Table>,

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
    /// Creates an instance from a module.
    pub fn instantiate<Mod, ExclArea>(
        module: &Mod,
        import_from: Vec<(&str, Instance<Area>)>,
        alloc: &impl MemoryAeaAllocator<Area = ExclArea>,
    ) -> ModuleResult<Self>
    where
        Mod: Module,
        ExclArea: ExclusiveMemoryArea<Shared = Area>,
    {
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
            FuncInfo::Native { ptr } => Ok(Func::Native { ptr: *ptr }),
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
        // Allocate heaps
        let heaps = module.heaps().try_map(|heap_info| match heap_info {
            HeapInfo::Owned { min_size, kind } => {
                let area = alloc
                    .with_capacity(*min_size as usize)
                    .map_err(|_| ModuleError::FailedToInstantiate)?;
                let heap = Heap::with_capacity(*min_size, *kind, area)
                    .map_err(|_| ModuleError::FailedToInstantiate)?;
                Ok(heap)
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

        // Allocate tables
        let tables = module.tables().try_map(|table_info| match table_info {
            crate::TableInfo::Owned { min_size, max_size } => {
                let table_size = if let Some(max_size) = max_size {
                    *max_size
                } else {
                    *min_size
                } as usize;
                let table = alloc::vec![0u64; table_size].into_boxed_slice();
                Ok(Table::Owned(table))
            }
            crate::TableInfo::Native { ptr } => Ok(Table::Owned(ptr.clone())),
            crate::TableInfo::Imported { module, name } => {
                // Look for the corresponding module
                let instance = &imports[*module];
                let table_ref = instance
                    .items
                    .get(name)
                    .ok_or(ModuleError::FailedToInstantiate)?
                    .as_table()
                    .ok_or(ModuleError::FailedToInstantiate)?;

                Ok(Table::Imported {
                    from: *module,
                    index: table_ref,
                })
            }
        })?;

        let items = module.public_items().clone();

        // Allocate code
        let mod_code = module.code();
        let mut code = alloc
            .with_capacity(mod_code.len())
            .map_err(|_| ModuleError::FailedToInstantiate)?;
        code.as_bytes_mut()[..mod_code.len()].copy_from_slice(mod_code);
        let code = code.into_shared();

        // Create instance
        let mut instance = Self {
            vmctx: VMContext::empty(module.vmctx_layout()),
            imports,
            items,
            heaps,
            tables,
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
    pub fn get_func_addr_by_name<'a, 'b>(&'a self, name: &'b str) -> Option<*const u8> {
        let name = self.items.get(name)?;
        let func = self.get_func_by_ref(*name)?;

        match func {
            Func::Owned { offset } => {
                let addr = self.code.as_ptr();

                // SAFETY: We rely on the function offset being correct here, in which case the offset is
                // less or equal to `code.len()` and points to the start of the intended function.
                unsafe { Some(addr.offset(*offset as isize)) }
            }
            Func::Imported { from, index: func } => todo!(),
            Func::Native { ptr } => Some(ptr.as_ptr()),
        }
    }

    /// Returns a table exported by the instance from it's exported name.
    pub fn get_table_by_name<'a, 'b>(&'a self, name: &'b str) -> Option<&Box<[u64]>> {
        let index = match self.items.get(name)? {
            ItemRef::Table(idx) => *idx,
            _ => return None,
        };
        Some(self.get_table(index))
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
            // SAFETY: This function is private and called just after instance initialization,
            // therefore we know that we are the sole owner of `self.code` at this point.
            //
            // TODO: refactor the code to remove need for unsafe?
            let code = unsafe { self.code.unsafe_as_bytes_mut() };
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

    /// Returns the address of a function.
    /// Imported functions are resolved through recursive lookups.
    fn get_func_ptr(&self, func: FuncIndex) -> *const u8 {
        match &self.funcs[func] {
            Func::Owned { offset } => self.code.as_ptr().wrapping_add(*offset as usize),
            Func::Imported { from, index } => {
                let instance = &self.imports[*from];
                instance.get_func_ptr(*index)
            }
            Func::Native { ptr } => ptr.as_ptr(),
        }
    }

    /// Returns the address of a heap.
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

    /// Returns a table.
    /// Imported tables are resolved through recursive lookups.
    fn get_table(&self, table: TableIndex) -> &Box<[u64]> {
        match &self.tables[table] {
            Table::Owned(table) => table,
            Table::Imported { from, index } => {
                let instance = &self.imports[*from];
                instance.get_table(*index)
            }
        }
    }

    /// Returns the address of a table.
    /// Imported tables are resolved through recursive lookups.
    ///
    /// TODO: for now we only support static bounds, i.e. tables can't be resized. Ideally, the
    /// bound should be a pointer to the location to which the bound is actually stored.
    fn get_table_ptr_and_bound(&self, table: TableIndex) -> (*const u8, usize) {
        let table = self.get_table(table);
        (table.as_ptr() as *const u8, table.len())
    }

    /// Returns the address of a global.
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
        for idx in self.tables.keys() {
            let (ptr, bound) = self.get_table_ptr_and_bound(idx);
            self.vmctx.set_table(ptr, bound, idx);
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

    /// Returns a function corresponding to the item reference, if that item is a function.
    fn get_func_by_ref(&self, item: ItemRef) -> Option<&Func> {
        match self.get_item_by_ref(item) {
            Item::Func(func) => Some(func),
            _ => None,
        }
    }

    /// Returns a table corresponding to the item reference, if that item is a table.
    fn get_table_by_ref(&self, item: ItemRef) -> Option<&Table> {
        match self.get_item_by_ref(item) {
            Item::Table(table) => Some(table),
            _ => None,
        }
    }

    /// Returns the item corresponding to the provided reference.
    fn get_item_by_ref(&self, item: ItemRef) -> Item<'_, Area> {
        match item {
            ItemRef::Func(idx) => Item::Func(&self.funcs[idx]),
            ItemRef::Heap(idx) => Item::Heap(&self.heaps[idx]),
            ItemRef::Table(idx) => Item::Table(&self.tables[idx]),
            ItemRef::Glob(idx) => todo!(),
            ItemRef::Import(idx) => todo!(),
        }
    }
}
