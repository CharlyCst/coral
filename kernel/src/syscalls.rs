//! Coral System Calls
//!
//! System Calls in Coral are provided as a native module, that can be linked to any Wasm module.

use crate::{kprint, kprintln};
use alloc::string::String;
use alloc::vec;
use core::mem;

use wasm::{ExternRef64, NativeModule, NativeModuleBuilder, RawFuncPtr};

// ————————————————————————————— Native Module —————————————————————————————— //

pub fn build_syscall_module() -> NativeModule {
    let table = vec![ExternRef::Buffer(BufferIndex(0))];
    unsafe {
        NativeModuleBuilder::new()
            .add_func(
                String::from("print_char"),
                RawFuncPtr::new(print_char as *mut u8),
            )
            .add_func(
                String::from("buffer_write"),
                RawFuncPtr::new(buffer_write as *mut u8),
            )
            .add_table(String::from("handles"), table)
            .build()
    }
}

// —————————————————————————————— System Calls —————————————————————————————— //

/// A WebAssembly externref.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ExternRef {
    Buffer(BufferIndex),
}

/// This value is used to assert a compile time that ExternRef is 8 bytes long.
#[doc(hidden)]
const _EXTERNREF_SIZE_ASSERT: [u8; 8] = [0; mem::size_of::<ExternRef>()];

impl ExternRef64 for ExternRef {
    fn to_u64(self) -> u64 {
        // SAFETY: transmute check for the size at compile time, and because all 64 values are
        // valid u64 the result is always valid.
        //
        // TODO: can we do that without transmute?
        unsafe { mem::transmute(self) }
    }
}

/// An index representing a buffer object.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct BufferIndex(u32);

/// The Virtual Machine Context, passed as argument to all instance functions, including native
/// functions.
type VmCtx = u64;

/// A WebAssembly u32.
type WasmU32 = u32;
/// A WebAssembly u64.
type WasmU64 = u64;

/// Prints a character.
///
/// The very first syscall! Useful for testing and debugging!
extern "sysv64" fn print_char(char: WasmU32, _vmctx: VmCtx) {
    if let Some(c) = char::from_u32(char) {
        kprint!("{}", c);
    }
}

extern "sysv64" fn buffer_write(
    handle: ExternRef,
    _buffer: WasmU64,
    _offset: WasmU64,
    _buffer_size: WasmU64,
) {
    kprintln!("Buffer Write: {:?}", handle);
}
