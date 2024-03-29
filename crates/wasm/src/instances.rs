use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;

use crate::traits::{
    DataSegment, FuncIndex, FuncInfo, FuncPtr, GlobIndex, GlobInfo, GlobInit, HeapIndex, HeapInfo,
    ImportIndex, ItemRef, MemoryArea, Module, ModuleError, ModuleResult, Reloc, RelocKind, Runtime,
    TableIndex, TypeIndex,
};
use crate::types::FuncType;
use crate::vmctx::VMContext;
use collections::{FrozenMap, HashMap};

const PAGE_SIZE: usize = 0x10000; // 64 Ki bytes

type Imports<Area> = FrozenMap<ImportIndex, Arc<Instance<Area>>>;

enum Item<'a, Area: MemoryArea> {
    Func(&'a Func),
    Heap(&'a Heap<Area>),
    Table(&'a Table),
}

enum Heap<Area> {
    Owned { memory: Area },
    Imported { from: ImportIndex, index: HeapIndex },
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
    Owned {
        offset: u32,
        ty: TypeIndex,
    },
    Imported {
        from: ImportIndex,
        index: FuncIndex,
        ty: TypeIndex,
    },
    Native {
        ptr: FuncPtr,
        ty: TypeIndex,
    },
}

impl Func {
    pub fn ty_index(&self) -> TypeIndex {
        match self {
            Func::Owned { ty, .. } => *ty,
            Func::Imported { ty, .. } => *ty,
            Func::Native { ty, .. } => *ty,
        }
    }
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
    imports: FrozenMap<ImportIndex, Arc<Instance<Area>>>,

    /// The function types used by the instance.
    types: FrozenMap<TypeIndex, FuncType>,

    /// The start function, if any.
    start: Option<FuncIndex>,

    /// The memory region containing the code
    code: Area,
}

impl<Area: MemoryArea> Instance<Area> {
    /// Creates an instance from a module.
    pub fn instantiate<Mod, Ctx>(
        module: &Mod,
        import_from: &[(&str, Arc<Instance<Area>>)],
        runtime: &impl Runtime<MemoryArea = Area, Context = Ctx>,
    ) -> ModuleResult<Self>
    where
        Mod: Module,
    {
        let mut ctx = runtime.create_context();
        let items = module.public_items().clone();
        let types = module.types().clone();

        let imports = Self::select_imports(module, &import_from)?;
        let funcs = Self::prepare_funcs(module, &imports, &types)?;
        let globs = Self::prepare_globs(module, &imports)?;
        let heaps = Self::allocate_heaps(module, &imports, runtime, &mut ctx)?;
        let tables = Self::allocate_tables(module, &imports, runtime, &mut ctx)?;
        let code = Self::allocate_code(module, &imports, &funcs, runtime, &mut ctx)?;

        // Create instance
        let mut instance = Self {
            vmctx: VMContext::empty(module.vmctx_layout()),
            start: module.start(),
            imports,
            items,
            heaps,
            tables,
            globs,
            funcs,
            types,
            code,
        };

        instance.init_tables(module);
        instance.init_vmctx(); // Set the VMContext to its expected initial values

        Ok(instance)
    }

    // ————————————————————————————— Instantiation —————————————————————————————— //

    /// Select the imports from the available instances.
    pub fn select_imports<Mod>(
        module: &Mod,
        import_from: &[(&str, Arc<Instance<Area>>)],
    ) -> ModuleResult<Imports<Area>>
    where
        Mod: Module,
    {
        module.imports().try_map(|module| {
            // Pick the first matching module
            for (name, instance) in import_from {
                if name == module {
                    return Ok(instance.clone());
                }
            }
            Err(ModuleError::FailedToInstantiate)
        })
    }

    fn prepare_funcs<Mod>(
        module: &Mod,
        imports: &Imports<Area>,
        types: &FrozenMap<TypeIndex, FuncType>,
    ) -> ModuleResult<FrozenMap<FuncIndex, Func>>
    where
        Mod: Module,
    {
        module.funcs().try_map(|func_info| match func_info {
            FuncInfo::Owned { offset, ty } => Ok(Func::Owned {
                offset: *offset,
                ty: *ty,
            }),
            FuncInfo::Native { ptr, ty } => Ok(Func::Native { ptr: *ptr, ty: *ty }),
            FuncInfo::Imported { module, name, ty } => {
                // Look for the corresponding module
                let instance = &imports[*module];
                let func_ref = instance
                    .items
                    .get(name)
                    .ok_or(ModuleError::FailedToInstantiate)?
                    .as_func()
                    .ok_or(ModuleError::FailedToInstantiate)?;

                // Typecheck function
                let my_type = &types[*ty];
                let other_func = &instance.funcs[func_ref];
                let other_type = &instance.types[other_func.ty_index()];
                if !my_type.eq(other_type) {
                    return Err(ModuleError::TypeError);
                }

                Ok(Func::Imported {
                    from: *module,
                    index: func_ref,
                    ty: *ty,
                })
            }
        })
    }

    fn prepare_globs<Mod>(
        module: &Mod,
        imports: &Imports<Area>,
    ) -> ModuleResult<FrozenMap<GlobIndex, Glob>>
    where
        Mod: Module,
    {
        module.globs().try_map(|glob_info| match glob_info {
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
        })
    }

    fn allocate_heaps<Mod, Ctx>(
        module: &Mod,
        imports: &Imports<Area>,
        runtime: &impl Runtime<MemoryArea = Area, Context = Ctx>,
        ctx: &mut Ctx,
    ) -> ModuleResult<FrozenMap<HeapIndex, Heap<Area>>>
    where
        Mod: Module,
    {
        module
            .heaps()
            .try_map_enumerate(|heap_idx, heap_info| match heap_info {
                HeapInfo::Owned { min_size, kind } => {
                    let mut initialized = false;
                    let initialize = |heap: &mut [u8]| {
                        if heap.len() < *min_size as usize {
                            return Err(ModuleError::FailedToInstantiate);
                        }
                        initialized = true;
                        Self::initialize_heap(heap, heap_idx, module.data_segments())
                    };

                    // Allocate heap
                    let area = runtime.alloc_heap(
                        (*min_size as usize) * PAGE_SIZE,
                        *kind,
                        initialize,
                        ctx,
                    )?;

                    // Check that the heap was initialized
                    if !initialized {
                        return Err(ModuleError::FailedToInstantiate);
                    }

                    Ok(Heap::Owned { memory: area })
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
            })
    }

    fn allocate_tables<Mod, Ctx>(
        module: &Mod,
        imports: &Imports<Area>,
        runtime: &impl Runtime<MemoryArea = Area, Context = Ctx>,
        ctx: &mut Ctx,
    ) -> ModuleResult<FrozenMap<TableIndex, Table>>
    where
        Mod: Module,
    {
        module.tables().try_map(|table_info| match table_info {
            crate::TableInfo::Owned {
                min_size,
                max_size,
                ty,
            } => {
                let table = runtime.alloc_table(*min_size, *max_size, *ty, ctx)?;
                Ok(Table::Owned(table))
            }
            crate::TableInfo::Native { ptr, .. } => Ok(Table::Owned(ptr.clone())),
            crate::TableInfo::Imported { module, name, .. } => {
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
        })
    }

    fn allocate_code<Mod, Ctx>(
        module: &Mod,
        imports: &Imports<Area>,
        funcs: &FrozenMap<FuncIndex, Func>,
        runtime: &impl Runtime<MemoryArea = Area, Context = Ctx>,
        ctx: &mut Ctx,
    ) -> ModuleResult<Area>
    where
        Mod: Module,
    {
        // Allocate code
        let mod_code = module.code();
        let relocs = module.relocs();
        let mut relocated = false;
        let relocate = |code: &mut [u8]| {
            if code.len() < mod_code.len() {
                return Err(ModuleError::FailedToInstantiate);
            }
            relocated = true;
            code[..mod_code.len()].copy_from_slice(mod_code);
            Self::relocate(code, relocs, &funcs, &imports)
        };
        let code = runtime.alloc_code(module.code().len(), relocate, ctx)?;
        if !relocated {
            // The runtime didn't properly relocate the code by calling the closure.
            return Err(ModuleError::RuntimeError);
        }
        Ok(code)
    }

    // ———————————————————————————————— Helpers ————————————————————————————————— //

    /// Returns the index of the start function, if any.
    pub fn get_start(&self) -> Option<FuncIndex> {
        self.start.clone()
    }

    /// Returns the address of the given function.
    pub fn get_func_addr_by_index(&self, index: FuncIndex) -> *const u8 {
        let func = &self.funcs[index];

        match func {
            Func::Owned { offset, .. } => {
                let addr = self.code.as_ptr();

                // SAFETY: We rely on the function offset being correct here, in which case the offset is
                // less or equal to `code.len()` and points to the start of the intended function.
                unsafe { addr.offset(*offset as isize) }
            }
            Func::Imported { .. } => todo!(),
            Func::Native { ptr, .. } => ptr.as_ptr(),
        }
    }

    /// Returns the type of the given function.
    pub fn get_func_type_by_index(&self, index: FuncIndex) -> &FuncType {
        let func = &self.funcs[index];
        let ty = match func {
            Func::Owned { ty, .. } => ty,
            Func::Imported { ty, .. } => ty,
            Func::Native { ty, .. } => ty,
        };
        &self.types[*ty]
    }

    /// Returns the index of a function exported by the instance.
    pub fn get_func_index_by_name<'a, 'b>(&'a self, name: &'b str) -> Option<FuncIndex> {
        match self.items.get(name) {
            Some(ItemRef::Func(index)) => Some(*index),
            _ => None,
        }
    }

    /// Returns the address of a function exported by the instance.
    pub fn get_func_addr_by_name<'a, 'b>(&'a self, name: &'b str) -> Option<*const u8> {
        let name = self.items.get(name)?;
        let func = self.get_func_by_ref(*name)?;

        match func {
            Func::Owned { offset, .. } => {
                let addr = self.code.as_ptr();

                // SAFETY: We rely on the function offset being correct here, in which case the offset is
                // less or equal to `code.len()` and points to the start of the intended function.
                unsafe { Some(addr.offset(*offset as isize)) }
            }
            Func::Imported { .. } => todo!(),
            Func::Native { ptr, .. } => Some(ptr.as_ptr()),
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

    fn initialize_heap(
        heap: &mut [u8],
        idx: HeapIndex,
        segments: &[DataSegment],
    ) -> ModuleResult<()> {
        // Zero out the memory
        heap.fill(0);

        // Initialize with data segments
        for segment in segments {
            // Skip segments for other heaps
            if segment.heap_index != idx {
                continue;
            }

            // Copy data
            let start = if let Some(_glob_idx) = segment.base {
                // TODO: handle globals
                todo!("Base are not yet supported for data segments");
            } else {
                segment.offset as usize
            };
            let end = start + segment.data.len();
            heap[start..end].copy_from_slice(&segment.data);
        }

        Ok(())
    }

    fn relocate(
        code: &mut [u8],
        relocs: &[Reloc],
        funcs: &FrozenMap<FuncIndex, Func>,
        imports: &Imports<Area>,
    ) -> ModuleResult<()> {
        for reloc in relocs {
            let base = match reloc.item {
                ItemRef::Func(func) => match &funcs[func] {
                    Func::Owned { offset, .. } => code.as_ptr().wrapping_add(*offset as usize),
                    Func::Imported { from, index, .. } => {
                        let instance = &imports[*from];
                        instance.get_func_ptr(*index)
                    }
                    Func::Native { ptr, .. } => ptr.as_ptr(),
                },
                // Only functions are supported by relocations
                _ => return Err(ModuleError::FailedToInstantiate),
            } as i64;
            let value = base + reloc.addend;

            let offset = reloc.offset as usize;
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
            Func::Owned { offset, .. } => self.code.as_ptr().wrapping_add(*offset as usize),
            Func::Imported { from, index, .. } => {
                let instance = &self.imports[*from];
                instance.get_func_ptr(*index)
            }
            Func::Native { ptr, .. } => ptr.as_ptr(),
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

    fn init_tables<Mod>(&mut self, module: &Mod)
    where
        Mod: Module,
    {
        for segment in module.table_segments() {
            let start = if let Some(_glob_idx) = segment.base {
                // TODO: handle globals
                todo!("Base are not yet supported for table segments");
            } else {
                segment.offset as usize
            };
            for (entry_idx, func_idx) in (start..).zip(segment.elements.iter()) {
                let ptr = self.get_func_ptr(*func_idx);
                match &mut self.tables[segment.table_index] {
                    Table::Owned(table) => table[entry_idx] = ptr as u64,
                    Table::Imported { .. } => panic!("Can't initialize imported tables"),
                };
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

    /// Returns the item corresponding to the provided reference.
    fn get_item_by_ref(&self, item: ItemRef) -> Item<'_, Area> {
        match item {
            ItemRef::Func(idx) => Item::Func(&self.funcs[idx]),
            ItemRef::Heap(idx) => Item::Heap(&self.heaps[idx]),
            ItemRef::Table(idx) => Item::Table(&self.tables[idx]),
            ItemRef::Glob(_) => todo!(),
            ItemRef::Import(_) => todo!(),
            ItemRef::Type(_) => todo!(),
        }
    }
}
