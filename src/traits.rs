use cranelift_codegen::entity::{entity_impl, Iter};

use core::alloc;

// ——————————————————————————————— Allocator ———————————————————————————————— //

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapKind {
    Static { max_size: u32 },
    Dynamic,
}

pub trait WriteXorExec {
    /// Set the memory to executable and remove the write permission.
    fn set_execute(&self, ptr: Box<[u8]>);
}

pub trait Allocator {
    type CodeAllocator: alloc::Allocator + WriteXorExec;
    type HeapAllocator: alloc::Allocator;

    /// Return a boxed slice of `code_size` writable bytes that can receive code.
    /// The code allocator is expected to respect a W^X (write XOr execute) policy, if that is the
    /// case the permissions must be switched to X before execution.
    fn alloc_code(&self, code_size: u32) -> Box<[u8], Self::CodeAllocator>;

    /// Return a boxed slice of at least `min_size` * PAGE_SIZE writable bytes to be used as heap.
    fn alloc_heap(
        &self,
        min_size: u32,
        max_size: Option<u32>,
        kind: HeapKind,
    ) -> Box<[u8], Self::HeapAllocator>;
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FuncIndex(u32);
entity_impl!(FuncIndex);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HeapIndex(u32);
entity_impl!(HeapIndex);

pub enum ModuleItem {
    Func(FuncIndex),
    Heap(HeapIndex),
}

pub struct FunctionInfo {
    pub offset: u32,
    // TODO: add signature
}

pub struct HeapInfo {
    pub min_size: u32,
    pub max_size: Option<u32>,
    pub kind: HeapKind,
}

/// The error that might occur during module instantiation.
#[derive(Debug)]
pub enum ModuleError {}

pub type ModuleResult<T> = Result<T, ModuleError>;

pub trait Module {
    fn code_len(&self) -> usize;
    fn code(&self) -> &[u8];
    fn heaps(&self) -> Iter<'_, HeapIndex, HeapInfo>;
    fn vmctx_items(&self) -> &[ModuleItem];
}
