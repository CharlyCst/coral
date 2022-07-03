use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use core::ops::Deref;
use core::ptr::NonNull;

use collections::{entity_impl, FrozenMap, HashMap};

// ——————————————————————————————— Allocator ———————————————————————————————— //

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapKind {
    Static { max_size: u32 },
    Dynamic,
}

/// A chunk of memory.
///
/// A memory area is associated with access permissions (Read, Write and Execute), therefore
/// special care must be taken when accessing it as it may throw an exception.
pub trait MemoryArea {
    /// Disables write and set execute permission.
    fn set_executable(&self);

    /// Disables execute and set write permission.
    fn set_write(&self);

    /// Disables execute and write permissions.
    fn set_read_only(&self);

    /// Returns a pointer to the begining of the area.
    fn as_ptr(&self) -> *const u8;

    /// Returns a mutable pointer to the begining of the area.
    fn as_mut_ptr(&self) -> *mut u8;

    /// Returns a view of the area.
    fn as_bytes(&self) -> &[u8];

    /// Returns a mutable view of the area.
    ///
    /// This functions is expected to be implemented with interior mutability, and therefore can be
    /// used by multiple instances that share a memory region. The responsibility of
    /// synchronization is deferred to the caller.
    ///
    /// SAFETY: The caller is responsible for ensuring that there is no concurrent access to the
    /// memory area.
    unsafe fn unsafe_as_bytes_mut(&self) -> &mut [u8];

    /// Returns the size of the area, in bytes.
    fn size(&self) -> usize;

    /// Extends the area by at least `n` bytes.
    fn extend_by(&self, n: usize) -> Result<(), ()>;

    /// Extends the area until it can hold at least `n` bytes.
    fn extend_to(&self, n: usize) -> Result<(), ()> {
        let size = self.size();
        if size < n {
            self.extend_by(size - n)
        } else {
            Ok(())
        }
    }
}

/// A memory area that is not yet (or no longer) shared.
pub trait ExclusiveMemoryArea: MemoryArea {
    type Shared: MemoryArea;

    /// Returns a mutable view of the area.
    fn as_bytes_mut(&mut self) -> &mut [u8];

    /// Returns a version of the area that can safely be shared.
    fn into_shared(self) -> Self::Shared;
}

impl<Area> MemoryArea for Arc<Area>
where
    Area: MemoryArea,
{
    #[inline]
    fn set_executable(&self) {
        self.deref().set_executable()
    }

    #[inline]
    fn set_write(&self) {
        self.deref().set_write()
    }

    #[inline]
    fn set_read_only(&self) {
        self.deref().set_read_only()
    }

    #[inline]
    fn as_ptr(&self) -> *const u8 {
        self.deref().as_ptr()
    }

    #[inline]
    fn as_mut_ptr(&self) -> *mut u8 {
        self.deref().as_mut_ptr()
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        self.deref().as_bytes()
    }

    #[inline]
    unsafe fn unsafe_as_bytes_mut(&self) -> &mut [u8] {
        self.deref().unsafe_as_bytes_mut()
    }

    #[inline]
    fn size(&self) -> usize {
        self.deref().size()
    }

    #[inline]
    fn extend_by(&self, n: usize) -> Result<(), ()> {
        self.deref().extend_by(n)
    }
}

/// An allocator that can allocate new memory areas.
pub trait MemoryAeaAllocator {
    type Area: ExclusiveMemoryArea;

    /// Allocates a memory area with read and write permissions and at least `capacity` bytes
    /// availables.
    fn with_capacity(
        &self,
        capacity: usize,
    ) -> Result<Self::Area, ()>;
}

// ————————————————————————————————— Module ————————————————————————————————— //

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct FuncIndex(u32);
entity_impl!(FuncIndex);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct HeapIndex(u32);
entity_impl!(HeapIndex);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct TableIndex(u32);
entity_impl!(TableIndex);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct GlobIndex(u32);
entity_impl!(GlobIndex);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ImportIndex(u32);
entity_impl!(ImportIndex);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum ItemRef {
    Func(FuncIndex),
    Heap(HeapIndex),
    Table(TableIndex),
    Glob(GlobIndex),
    Import(ImportIndex),
}

impl ItemRef {
    pub fn as_func(self) -> Option<FuncIndex> {
        match self {
            ItemRef::Func(idx) => Some(idx),
            _ => None,
        }
    }

    pub fn as_heap(self) -> Option<HeapIndex> {
        match self {
            ItemRef::Heap(idx) => Some(idx),
            _ => None,
        }
    }

    pub fn as_table(self) -> Option<TableIndex> {
        match self {
            ItemRef::Table(idx) => Some(idx),
            _ => None,
        }
    }

    pub fn as_glob(self) -> Option<GlobIndex> {
        match self {
            ItemRef::Glob(idx) => Some(idx),
            _ => None,
        }
    }
}

/// A raw function pointer.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct RawFuncPtr(NonNull<u8>);

impl RawFuncPtr {
    /// Creates a raw function pointer.
    ///
    /// SAFETY: Note that the pointer might be used to call the function from Wasm Instances, and
    /// therefore the function must respect the adequate calling convention. At the time of
    /// writing, this means SystemV calling convention with a `vmctx: u64` as last argument.
    /// Note that the calling convention might be subject to change, there are no stability
    /// guarantees yet!
    pub unsafe fn new(func_ptr: *mut u8) -> Self {
        Self(NonNull::new(func_ptr).unwrap())
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }
}

pub trait ExternRef64: Copy {
    fn to_u64(self) -> u64;
}

pub enum FuncInfo {
    // TODO: add signatures
    Owned { offset: u32 },
    Imported { module: ImportIndex, name: String },
    Native { ptr: RawFuncPtr },
}

impl FuncInfo {
    pub fn is_imported(&self) -> bool {
        match self {
            FuncInfo::Imported { .. } => true,
            FuncInfo::Owned { .. } => false,
            FuncInfo::Native { .. } => false,
        }
    }
}

pub enum HeapInfo {
    Owned { min_size: u32, kind: HeapKind },
    Imported { module: ImportIndex, name: String },
}

pub enum TableInfo {
    Owned {
        min_size: u32,
        max_size: Option<u32>,
    },
    Imported {
        module: ImportIndex,
        name: String,
    },
    Native {
        ptr: Box<[u64]>,
    },
}

/// Possible initial values for a global variable.
#[derive(Clone, Copy)]
pub enum GlobInit {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
}

pub enum GlobInfo {
    // TODO: add type
    Owned { init: GlobInit },
    Imported { module: ImportIndex, name: String },
}

pub trait VMContextLayout {
    fn heaps(&self) -> &[HeapIndex];
    fn tables(&self) -> &[TableIndex];
    fn funcs(&self) -> &[FuncIndex];
    fn globs(&self) -> &[GlobIndex];
    fn imports(&self) -> &[ImportIndex];
}

/// One to one mapping to Cranelift `Reloc`. See Cranelift for details.
pub enum RelocKind {
    Abs4,
    Abs8,
    X86PCRel4,
    X86CallPCRel4,
    X86CallPLTRel4,
    X86GOTPCRel4,
    Arm32Call,
    Arm64Call,
    S390xPCRel32Dbl,
    ElfX86_64TlsGd,
    MachOX86_64Tlv,
    Aarch64TlsGdAdrPage21,
    Aarch64TlsGdAddLo12Nc,
}

/// Addend to add to the symbol value.
pub type Addend = i64;

pub struct Reloc {
    /// Offset of the relocation, relative to the module's code address.
    pub offset: u32,

    /// The kind of relocation.
    //  TODO: abstract over cranelift_codegen to avoid pulling in the dependency.
    pub kind: RelocKind,

    /// The symbol, whose address corresponds to the new relocation value.
    pub item: ItemRef,

    /// A value to add to the relocation.
    pub addend: Addend,
}

/// The error that might occur during module instantiation.
#[derive(Debug)]
pub enum ModuleError {
    FailedToInstantiate,
}

pub type ModuleResult<T> = Result<T, ModuleError>;

pub trait Module {
    type VMContext: VMContextLayout + Clone + 'static;

    fn code(&self) -> &[u8];
    fn heaps(&self) -> &FrozenMap<HeapIndex, HeapInfo>;
    fn tables(&self) -> &FrozenMap<TableIndex, TableInfo>;
    fn funcs(&self) -> &FrozenMap<FuncIndex, FuncInfo>;
    fn globs(&self) -> &FrozenMap<GlobIndex, GlobInfo>;
    fn imports(&self) -> &FrozenMap<ImportIndex, String>;
    fn relocs(&self) -> &[Reloc];
    fn public_items(&self) -> &HashMap<String, ItemRef>;
    fn vmctx_layout(&self) -> &Self::VMContext;
}
