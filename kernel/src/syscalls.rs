//! Coral System Calls
//!
//! System Calls in Coral are provided as a native module, that can be linked to any Wasm module.

use crate::kprint;
use alloc::string::String;

use wasm::{NativeModule, NativeModuleBuilder, RawFuncPtr};

// ————————————————————————————— Native Module —————————————————————————————— //

pub fn build_syscall_module() -> NativeModule {
    unsafe {
        NativeModuleBuilder::new()
            .add_func(
                String::from("print_char"),
                RawFuncPtr::new(print_char as *mut u8),
            )
            .build()
    }
}

// —————————————————————————————— System Calls —————————————————————————————— //

/// The Virtual Machine Context, passed as argument to all instance functions, including native
/// functions.
type VmCtx = u64;

/// A WebAssembly u32.
type WasmU32 = u32;

/// Prints a character.
///
/// The very first syscall! Useful for testing and debugging!
extern "sysv64" fn print_char(char: WasmU32, _vmctx: VmCtx) {
    if let Some(c) = char::from_u32(char) {
        kprint!("{}", c);
    }
}
