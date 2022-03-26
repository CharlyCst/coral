use core::alloc;

pub use cranelift_codegen::binemit::Addend;
pub use cranelift_codegen::binemit::Reloc as RelocKind;

use crate::collections::{entity_impl, FrozenMap, HashMap};

// ——————————————————————————————— Allocator ———————————————————————————————— //

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapKind {
    Static { max_size: u32 },
    Dynamic,
}

pub trait Allocator {
    type CodeAllocator: alloc::Allocator;
    type HeapAllocator: alloc::Allocator;

    /// Return a boxed slice of `code_size` writable bytes that can receive code.
    /// The code allocator is expected to respect a W^X (write XOr execute) policy, if that is the
    /// case the permissions must be switched to X before execution.
    fn alloc_code(&self, code_size: u32) -> Box<[u8], Self::CodeAllocator>;
    fn set_executable(&self, ptr: &Box<[u8], Self::CodeAllocator>);

    /// Return a boxed slice of at least `min_size` * PAGE_SIZE writable bytes to be used as heap.
    fn alloc_heap(&self, min_size: u32, kind: HeapKind) -> Box<[u8], Self::HeapAllocator>;
}

// ———————————————————————————————— Compiler ———————————————————————————————— //

/// The errors that might occur during compilation.
///
/// TODO: collect cummulated errors.
/// NOTE: We don't want to allocate in the error path as any allocation can fail.
#[derive(Debug)]
pub enum CompilerError {
    FailedToParse,
    FailedToCompile,
}

pub type CompilerResult<T> = Result<T, CompilerError>;

pub trait Compiler {
    type Module;

    fn parse(&mut self, wasm_bytecode: &[u8]) -> CompilerResult<()>;
    fn compile(self) -> CompilerResult<Self::Module>;
}

// ————————————————————————————————— Module ————————————————————————————————— //

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct FuncIndex(u32);
entity_impl!(FuncIndex);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct HeapIndex(u32);
entity_impl!(HeapIndex);

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

    pub fn as_glob(self) -> Option<GlobIndex> {
        match self {
            ItemRef::Glob(idx) => Some(idx),
            _ => None,
        }
    }
}

pub enum FuncInfo {
    // TODO: add signatures
    Owned { offset: u32 },
    Imported { module: ImportIndex, name: String },
}

impl FuncInfo {
    pub fn is_imported(&self) -> bool {
        match self {
            FuncInfo::Imported { .. } => true,
            FuncInfo::Owned { .. } => false,
        }
    }
}

pub enum HeapInfo {
    Owned { min_size: u32, kind: HeapKind },
    Imported { module: ImportIndex, name: String },
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
    fn funcs(&self) -> &[FuncIndex];
    fn globs(&self) -> &[GlobIndex];
    fn imports(&self) -> &[ImportIndex];
}

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
    fn funcs(&self) -> &FrozenMap<FuncIndex, FuncInfo>;
    fn globs(&self) -> &FrozenMap<GlobIndex, GlobInfo>;
    fn imports(&self) -> &FrozenMap<ImportIndex, String>;
    fn relocs(&self) -> &[Reloc];
    fn public_items(&self) -> &HashMap<String, ItemRef>;
    fn vmctx_layout(&self) -> &Self::VMContext;
}
