// ———————————————————————————— Module Allocator ———————————————————————————— //

pub trait ModuleAllocator {
    /// Return a raw pointer to a memory region suitable for receiving `code_size` bytes of
    /// executable code.
    fn alloc_code(&mut self, code_size: usize) -> *mut u8;
    fn alloc_memory(&mut self);
    /// Terminate the module allocation.
    ///
    /// This function is expected to set back protections for the code segment.
    fn terminate(self);
}

// ———————————————————————————————— Compiler ———————————————————————————————— //

/// The errors that might append during compilation.
///
/// TODO: collect commulated errors.
/// NOTE: We don't want to allocate in the error path as any allocation can fail.
#[derive(Debug)]
pub enum CompilerError {
    FailedToParse,
}

pub type CompilerResults<T> = Result<T, CompilerError>;

pub trait Compiler {
    type Module;

    fn parse(&mut self, wasm_bytecode: &[u8]) -> CompilerResults<()>;
    fn compile<Alloc>(self, alloc: &mut Alloc) -> CompilerResults<Self::Module>
    where
        Alloc: ModuleAllocator;
}
