//! Coral System Calls
//!
//! System Calls in Coral are provided as a native module, that can be linked to any Wasm module.

use alloc::string::String;
use alloc::vec::Vec;

use crate::{kprint, kprintln};
use wasm::{MemoryArea, NativeModule, NativeModuleBuilder, RawFuncPtr};

mod kernel_objects;

pub use kernel_objects::{ExternRef, VmaIndex, ACTIVE_VMA, KoIndex};

// ————————————————————————————— Native Module —————————————————————————————— //

/// Build a native module exposing all the Coral system calls.
pub fn build_syscall_module(handles_table: Vec<ExternRef>) -> NativeModule {
    unsafe {
        NativeModuleBuilder::new()
            .add_func(
                String::from("print_char"),
                RawFuncPtr::new(print_char as *mut u8),
            )
            .add_func(
                String::from("buffer_write"),
                RawFuncPtr::new(vma_write as *mut u8),
            )
            .add_table(String::from("handles"), handles_table)
            .build()
    }
}

// ————————————————————————————————— Types —————————————————————————————————— //

/// The Virtual Machine Context, passed as argument to all instance functions, including native
/// functions.
type VmCtx = u64;

/// A WebAssembly u32.
type WasmU32 = u32;
/// A WebAssembly u64.
type WasmU64 = u64;

// —————————————————————————————— System Calls —————————————————————————————— //

/// Prints a character.
///
/// The very first syscall! Useful for testing and debugging!
extern "sysv64" fn print_char(char: WasmU32, _vmctx: VmCtx) {
    if let Some(c) = char::from_u32(char) {
        kprint!("{}", c);
    }
}

extern "sysv64" fn vma_write(
    handle: ExternRef,
    buffer: WasmU64,
    vma_offset: WasmU64,
    buffer_size: WasmU64,
    _vmctx: VmCtx,
) {
    kprintln!(
        "Buffer Write: {:?} - address 0x{:x} - offset 0x{:x} - len 0x{:x}",
        handle,
        buffer,
        vma_offset,
        buffer_size
    );
    let idx = match handle {
        ExternRef::Vma(vma_idx) => vma_idx,
    };
    let vma = match ACTIVE_VMA.get(idx) {
        Some(vma) => vma,
        None => return,
    };

    // SAFETY: TODO: what are the safety condition? Assume that userspace synchronized correctly?
    unsafe {
        let _buffer = vma.unsafe_as_bytes_mut();
        // TODO: get the instance memory
        // buffer[..vma_offset][..buffer_size]
    }
}
