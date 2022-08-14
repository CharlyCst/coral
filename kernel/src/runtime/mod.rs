//! WebAssembly Runtime
//!
//! This module provides the necessary runtime support for proper instantiation and execution of
//! userspace modules, as well as support for managing kernel objects.

mod kernel_objects;
mod runtime;

pub use kernel_objects::{KoIndex, ModuleIndex, VmaIndex, ACTIVE_MODULES, ACTIVE_VMA};
pub use runtime::Runtime;

use alloc::boxed::Box;
use conquer_once::OnceCell;

use wasm::WasmModule;

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
