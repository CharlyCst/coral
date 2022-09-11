//! Coral System Calls
//!
//! System Calls in Coral are provided as a native module, that can be linked to any Wasm module.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::mem;

use crate::memory::Vma;
use crate::runtime::compile;
use crate::runtime::{
    ComponentIndex, KoIndex, ModuleIndex, VmaIndex, ACTIVE_COMPONENTS, ACTIVE_MODULES, ACTIVE_VMA,
};
use crate::wasm::Component;
use wasm::{as_native_func, ExternRef64, NativeModule, NativeModuleBuilder, WasmModule, WasmType};

// ————————————————————————————— Native Module —————————————————————————————— //

/// Build a native module exposing all the Coral system calls.
pub fn build_syscall_module(handles_table: Vec<ExternRef>) -> NativeModule {
    unsafe {
        NativeModuleBuilder::new()
            .add_func(String::from("handle_kind"), &HANDLE_KIND)
            .add_func(String::from("vma_write"), &VMA_WRITE)
            .add_func(String::from("module_create"), &MODULE_CREATE)
            .add_func(String::from("component_create"), &COMPONENT_CREATE)
            .add_func(
                String::from("component_add_instance"),
                &COMPONENT_ADD_INSTANCE,
            )
            .add_table(String::from("handles"), handles_table)
            .build()
    }
}

// ————————————————————————————————— Types —————————————————————————————————— //

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
    /// A component.
    Component(ComponentIndex),
}

/// This value is used to assert a compile time that ExternRef is 8 bytes long.
#[doc(hidden)]
const _EXTERNREF_SIZE_ASSERT: [u8; 8] = [0; mem::size_of::<ExternRef>()];

unsafe impl WasmType for ExternRef {
    type Abi = ExternRef64;

    fn into_abi(self) -> u64 {
        // SAFETY: All valid ExternRef are valid u64.
        unsafe { mem::transmute(self) }
    }

    fn from_abi(val: u64) -> Self {
        // SAFETY: as WebAssembly can not modifie or create new reference values, all the values
        // received here are emitted by `Self::into_abi`.
        unsafe { mem::transmute(val) }
    }
}

// —————————————————————————————— Return Types —————————————————————————————— //

#[derive(Clone, Copy)]
#[repr(i32)]
pub enum SyscallResult {
    Success = 0,
    InvalidParams = 1,
    InternalError = 2,
    UnknownError = 3,
}

unsafe impl WasmType for SyscallResult {
    type Abi = i32;

    fn into_abi(self) -> Self::Abi {
        self as i32
    }

    fn from_abi(_val: Self::Abi) -> Self {
        // Do we need that conversion? Userspace has no reason to send errors
        todo!();
    }
}

#[derive(Clone, Copy)]
#[repr(u32)]
pub enum HandleKind {
    Invalid = 0,
    Vma = 1,
    Module = 2,
    Component = 3,
}

unsafe impl WasmType for HandleKind {
    type Abi = u32;

    fn into_abi(self) -> <Self::Abi as wasm::WasmBaseType>::Abi {
        self as u32
    }

    fn from_abi(_val: <Self::Abi as wasm::WasmBaseType>::Abi) -> Self {
        // We should never need that conversion
        todo!();
    }
}

// —————————————————————————————— System Calls —————————————————————————————— //

as_native_func!(handle_kind; HANDLE_KIND; args: ExternRef; ret: HandleKind);
fn handle_kind(handle: ExternRef) -> HandleKind {
    match handle {
        ExternRef::Invalid => HandleKind::Invalid,
        ExternRef::Vma(_) => HandleKind::Vma,
        ExternRef::Module(_) => HandleKind::Module,
        ExternRef::Component(_) => HandleKind::Component,
    }
}

as_native_func!(module_create; MODULE_CREATE; args: ExternRef u64 u64; ret: (SyscallResult, ExternRef));
fn module_create(source: ExternRef, offset: u64, size: u64) -> (SyscallResult, ExternRef) {
    let source_vma = match get_vma(source) {
        Ok(vma) => vma,
        Err(err) => return (err, ExternRef::Invalid),
    };

    let source = match vma_as_buf(&source_vma, offset, size) {
        Ok(buf) => buf,
        Err(err) => return (err, ExternRef::Invalid),
    };

    let module = match compile(&source) {
        Ok(module) => Arc::new(module),
        Err(_) => return (SyscallResult::InvalidParams, ExternRef::Invalid),
    };

    let handle = ACTIVE_MODULES.insert(module).into_externref();
    (SyscallResult::Success, handle)
}

as_native_func!(component_create; COMPONENT_CREATE; ret: (SyscallResult, ExternRef));
fn component_create() -> (SyscallResult, ExternRef) {
    let component = Arc::new(Component::new());
    let handle = ACTIVE_COMPONENTS.insert(component).into_externref();
    (SyscallResult::Success, handle)
}

as_native_func!(
    component_add_instance;
    COMPONENT_ADD_INSTANCE;
    args: ExternRef ExternRef;
    ret: (SyscallResult, u32)
);
fn component_add_instance(component: ExternRef, module: ExternRef) -> (SyscallResult, u32) {
    let component = match get_component(component) {
        Ok(component) => component,
        Err(err) => return (err, 0),
    };

    let module = match get_module(module) {
        Ok(module) => module,
        Err(err) => return (err, 0),
    };

    match component.add_instance(module.as_ref()) {
        Ok(idx) => (SyscallResult::Success, idx.as_u32()),
        Err(_) => (SyscallResult::InvalidParams, 0),
    }
}

as_native_func!(vma_write; VMA_WRITE; args: ExternRef ExternRef u64 u64 u64; ret: SyscallResult);
fn vma_write(
    source: ExternRef,
    target: ExternRef,
    source_offset: u64,
    target_offset: u64,
    size: u64,
) -> SyscallResult {
    let source_vma = match get_vma(source) {
        Ok(vma) => vma,
        Err(err) => return err,
    };
    let mut target_vma = match get_vma(target) {
        Ok(vma) => vma,
        Err(err) => return err,
    };

    let source = match vma_as_buf(&source_vma, source_offset, size) {
        Ok(buf) => buf,
        Err(err) => return err,
    };
    let target = match vma_as_buf_mut(&mut target_vma, target_offset, size) {
        Ok(buf) => buf,
        Err(err) => return err,
    };

    target.copy_from_slice(source);
    SyscallResult::Success
}

// ————————————————————————————————— Utils —————————————————————————————————— //

/// Returns the component corresponding to the given handle, if any.
fn get_component(handle: ExternRef) -> Result<Arc<Component>, SyscallResult> {
    let component_idx = match handle {
        ExternRef::Component(component) => component,
        _ => {
            crate::kprintln!("Syscall Error: expected component, got '{:?}'", handle);
            return Err(SyscallResult::InvalidParams);
        }
    };
    match ACTIVE_COMPONENTS.get(component_idx) {
        Some(component) => Ok(component),
        None => {
            crate::kprintln!("Syscall Error: component does not exists");
            Err(SyscallResult::InvalidParams)
        }
    }
}

/// Returns the module corresponding to the given handle, if any.
fn get_module(handle: ExternRef) -> Result<Arc<WasmModule>, SyscallResult> {
    let module_idx = match handle {
        ExternRef::Module(module) => module,
        _ => {
            crate::kprintln!("Syscall Error: expected module , got '{:?}'", handle);
            return Err(SyscallResult::InvalidParams);
        }
    };
    match ACTIVE_MODULES.get(module_idx) {
        Some(module) => Ok(module),
        None => {
            crate::kprintln!("Syscall Error: component does not exists");
            Err(SyscallResult::InvalidParams)
        }
    }
}

/// Returns the VMA corresponding to the given handle, if any.
fn get_vma(handle: ExternRef) -> Result<Arc<Vma>, SyscallResult> {
    let vma_idx = match handle {
        ExternRef::Vma(vma) => vma,
        _ => {
            crate::kprintln!("Syscall Error: expected VMA, got {:?}", handle);
            return Err(SyscallResult::InvalidParams);
        }
    };
    match ACTIVE_VMA.get(vma_idx) {
        Some(vma) => Ok(vma),
        None => {
            crate::kprintln!("Syscall Error: VMA does not exists");
            Err(SyscallResult::InvalidParams)
        }
    }
}

/// Returns a view of the given VMA at the given offset and with the given size.
fn vma_as_buf(vma: &Vma, offset: u64, size: u64) -> Result<&[u8], SyscallResult> {
    // TODO: handle permissions here
    let offset = usize::try_from(offset).map_err(|_| SyscallResult::InvalidParams)?;
    let size = usize::try_from(size).map_err(|_| SyscallResult::InvalidParams)?;
    let end = match offset.checked_add(size) {
        Some(end) => end,
        None => return Err(SyscallResult::InvalidParams),
    };

    let buf = vma.as_bytes();
    if buf.len() < end {
        Err(SyscallResult::InvalidParams)
    } else {
        Ok(&buf[offset..end])
    }
}

/// Returns a mutable view of the given VMA at the given offset and with the given size.
fn vma_as_buf_mut(vma: &mut Arc<Vma>, offset: u64, size: u64) -> Result<&mut [u8], SyscallResult> {
    // TODO: handle permissions here
    let offset = usize::try_from(offset).map_err(|_| SyscallResult::InvalidParams)?;
    let size = usize::try_from(size).map_err(|_| SyscallResult::InvalidParams)?;
    let end = match offset.checked_add(size) {
        Some(end) => end,
        None => return Err(SyscallResult::InvalidParams),
    };

    // TODO: what are the safety conditions here?
    let buf = unsafe { vma.unsafe_as_bytes_mut() };
    if buf.len() < end {
        Err(SyscallResult::InvalidParams)
    } else {
        Ok(&mut buf[offset..end])
    }
}
