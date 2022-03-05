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
pub struct ImportIndex(u32);
entity_impl!(ImportIndex);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemRef {
    Func(FuncIndex),
    Heap(HeapIndex),
}

impl ItemRef {
    pub fn as_func(self) -> Option<FuncIndex> {
        match self {
            ItemRef::Func(idx) => Some(idx),
            _ => None
        }
    }
}

pub enum FuncInfo {
    // TODO: add signatures
    Owned {
        offset: u32,
    },
    Imported {
        module: String,
        name: String,
    }
}

pub struct HeapInfo {
    pub min_size: u32,
    pub max_size: Option<u32>,
    pub kind: HeapKind,
}

pub struct Reloc {
    /// Offset of the relocation, relative to the module's code address.
    pub offset: u32,

    /// The kind of relocation.
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
    fn code(&self) -> &[u8];
    fn heaps(&self) -> &FrozenMap<HeapIndex, HeapInfo>;
    fn funcs(&self) -> &FrozenMap<FuncIndex, FuncInfo>;
    fn relocs(&self) -> &[Reloc];
    fn public_items(&self) -> &HashMap<String, ItemRef>;
    fn vmctx_items(&self) -> &[ItemRef];
}
