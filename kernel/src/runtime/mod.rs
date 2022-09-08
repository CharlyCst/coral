//! WebAssembly Runtime
//!
//! This module provides the necessary runtime support for proper instantiation and execution of
//! userspace modules, as well as support for managing kernel objects.

mod kernel_objects;
mod runtime;

use crate::memory::VmaAllocator;
pub use kernel_objects::{
    ComponentIndex, KoIndex, ModuleIndex, VmaIndex, ACTIVE_COMPONENTS, ACTIVE_MODULES, ACTIVE_VMA,
};
pub use runtime::Runtime;

use alloc::boxed::Box;
use conquer_once::OnceCell;

use wasm::WasmModule;

// ————————————————————————————— Global Runtime ————————————————————————————— //

static RUNTIME: OnceCell<Runtime> = OnceCell::uninit();

/// Initializes the global runtime.
///
/// This is required before instantiating and running WebAssembly instances.
pub fn init(alloc: VmaAllocator) {
    RUNTIME
        .try_init_once(|| Runtime::new(alloc))
        .expect("The runtime must be initialized only once");
}

/// Returns the global runtime.
///
/// This operation panics if the runtime has not yet been initialized.
pub fn get_runtime() -> &'static Runtime {
    match RUNTIME.try_get() {
        Ok(runtime) => runtime,
        Err(_) => {
            panic!("The runtime must be initialized before instantiating or calling WebAssembly modules")
        }
    }
}

// ——————————————————————— Optionnal Compiler Support ——————————————————————— //

type CompilerClosure = Box<dyn Fn(&[u8]) -> Result<WasmModule, ()> + Send + Sync>;

static COMPILER: OnceCell<CompilerClosure> = OnceCell::uninit();

pub fn register_compiler(closure: CompilerClosure) {
    COMPILER
        .try_init_once(|| closure)
        .expect("The compiler must be registered only once");
}

pub fn compile(wasm: &[u8]) -> Result<WasmModule, ()> {
    let compiler = match COMPILER.try_get() {
        Ok(compiler) => compiler,
        Err(_) => {
            crate::kprintln!("No compiler registered");
            return Err(());
        }
    };
    compiler(wasm)
}
