// ——————————————————————————————— Allocator ———————————————————————————————— //

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapKind {
    Static,
    Dynamic,
}

// TODO: switch to references instead of raw pointers
pub trait Allocator {
    /// Return a raw pointer to a memory region suitable for receiving `code_size` bytes of
    /// executable code.
    fn alloc_code(&mut self, code_size: u32) -> *mut u8;
    /// Return a raw pointer to a heap memory region.
    fn alloc_heap(&mut self, min_size: u32, max_size: Option<u32>, kind: HeapKind) -> *mut u8;
    /// Terminate the module allocation.
    ///
    /// This function is expected to set back protections for the code segment.
    fn terminate(self);
}

// ———————————————————————————————— Compiler ———————————————————————————————— //

/// The errors that might occur during compilation.
///
/// TODO: collect commulated errors.
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

/// The error that might occur during module instantiation.
#[derive(Debug)]
pub enum ModuleError {
    FailedToInstantiate,
}

pub type ModuleResult<T> = Result<T, ModuleError>;

pub trait Module {
    type Instance;

    fn instantiate<Alloc>(&self, alloc: &mut Alloc) -> ModuleResult<Self::Instance>
    where
        Alloc: Allocator;
}
