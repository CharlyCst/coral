//! Coral System Calls
//!
//! System Calls in Coral are provided as a native module, that can be linked to any Wasm module.

use alloc::string::String;
use alloc::vec::Vec;
use core::mem;

use crate::runtime::{VmaIndex, ACTIVE_VMA};
use crate::{kprint, kprintln};
use wasm::{ExternRef64, NativeModule, NativeModuleBuilder, RawFuncPtr};

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
                String::from("vma_write"),
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

/// A WebAssembly externref.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ExternRef {
    /// An invalid handle.
    Invalid,
    /// A virtual memory area.
    Vma(VmaIndex),
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
    source: ExternRef,
    target: ExternRef,
    source_offset: WasmU64,
    target_offset: WasmU64,
    size: WasmU64,
    _vmctx: VmCtx,
) {
    kprintln!(
        "VMA-Write - source {:?} + 0x{:x} - target {:?} + 0x{:x} - len 0x{:x}",
        source,
        source_offset,
        target,
        target_offset,
        size,
    );
    let source = match source {
        ExternRef::Vma(vma_idx) => vma_idx,
        _ => todo!("Source handle is invalid"), // Return an error
    };
    let target = match target {
        ExternRef::Vma(vma_idx) => vma_idx,
        _ => todo!("Target handle is invalid"),
    };
    let source_vma = match ACTIVE_VMA.get(source) {
        Some(vma) => vma,
        None => todo!("Source VMA does not exist"),
    };
    let target_vma = match ACTIVE_VMA.get(target) {
        Some(vma) => vma,
        None => todo!("Target VMA does not exist"),
    };

    let size = usize::try_from(size).expect("Invalid size");
    let source_offset = usize::try_from(source_offset).expect("Invalid source offset");
    let target_offset = usize::try_from(target_offset).expect("Invalid target offset");

    // SAFETY: TODO: what are the safety condition? Assume that userspace synchronized correctly?
    unsafe {
        let source = source_vma.unsafe_as_bytes_mut();
        let target = target_vma.unsafe_as_bytes_mut();

        let source_end = match source_offset.checked_add(size) {
            Some(source_end) => source_end,
            None => todo!("Invalid source range"),
        };
        let target_end = match target_offset.checked_add(size) {
            Some(target_end) => target_end,
            None => todo!("Invalid source range"),
        };
        if source.len() < source_end {
            todo!("Source index out of bound");
        }
        if target.len() < target_end {
            todo!("Target index out of bound");
        }
        target[target_offset..target_end].copy_from_slice(&source[source_offset..source_end]);
    }
}
