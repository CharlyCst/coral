//! Coral System Calls
//!
//! System Calls in Coral are provided as a native module, that can be linked to any Wasm module.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;

use crate::runtime::{compile, KoIndex, ModuleIndex, VmaIndex, ACTIVE_MODULES, ACTIVE_VMA};
use wasm::{ExternRef64, FuncPtr, FuncType, NativeModule, NativeModuleBuilder, ValueType};

// ————————————————————————————— Native Module —————————————————————————————— //

/// Build a native module exposing all the Coral system calls.
pub fn build_syscall_module(handles_table: Vec<ExternRef>) -> NativeModule {
    unsafe {
        NativeModuleBuilder::new()
            .add_func(
                String::from("vma_write"),
                FuncPtr::new(vma_write as *mut u8),
                FuncType::new(
                    vec![
                        ValueType::ExternRef,
                        ValueType::ExternRef,
                        ValueType::I64,
                        ValueType::I64,
                        ValueType::I64,
                    ],
                    vec![],
                ),
            )
            .add_func(
                String::from("module_create"),
                FuncPtr::new(module_create as *mut u8),
                FuncType::new(
                    vec![ValueType::ExternRef, ValueType::I64, ValueType::I64],
                    vec![ValueType::ExternRef],
                ),
            )
            .add_table(String::from("handles"), handles_table)
            .build()
    }
}

// ————————————————————————————————— Types —————————————————————————————————— //

/// The Virtual Machine Context, passed as argument to all instance functions, including native
/// functions.
type VmCtx = u64;

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
    /// A WebAssembly module.
    Module(ModuleIndex),
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

extern "sysv64" fn module_create(
    source: ExternRef,
    offset: WasmU64,
    size: WasmU64,
    _vmctx: VmCtx,
) -> ExternRef {
    let source = match source {
        ExternRef::Vma(vma_idx) => vma_idx,
        _ => todo!("Source handle is invalid"),
    };
    let source_vma = match ACTIVE_VMA.get(source) {
        Some(vma) => vma,
        None => todo!("Source VMA does not exist"),
    };

    let size = usize::try_from(size).expect("Invalid size");
    let offset = usize::try_from(offset).expect("Invalid offset");

    let source = source_vma.as_bytes();
    let end = match offset.checked_add(size) {
        Some(end) => end,
        None => todo!("Invalid source range"),
    };
    if source.len() < end {
        todo!("Source index out of bound");
    }

    let module = match compile(&source[offset..end]) {
        Ok(module) => Arc::new(module),
        Err(_) => todo!("Module failed to compile"),
    };

    ACTIVE_MODULES.insert(module).into_externref()
}

extern "sysv64" fn vma_write(
    source: ExternRef,
    target: ExternRef,
    source_offset: WasmU64,
    target_offset: WasmU64,
    size: WasmU64,
    _vmctx: VmCtx,
) {
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
