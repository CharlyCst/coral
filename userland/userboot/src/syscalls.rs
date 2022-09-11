//! Coral System Calls
#![allow(improper_ctypes)]

type ExternRef = u32;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Component(u32);

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Module(u32);

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct SyscallResult(pub i32);

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct InstanceIndex(u32);

impl SyscallResult {
    pub fn str(self) -> &'static str {
        match self.0 {
            0 => "Success",
            1 => "Invalid Params",
            2 => "Internal Error",
            _ => "Unkonwn Error",
        }
    }
}

#[link(wasm_import_module = "coral")]
extern "C" {
    pub fn vma_write(
        source: ExternRef,
        target: ExternRef,
        source_offset: u64,
        target_offset: u64,
        size: u64,
    ) -> SyscallResult;

    pub fn module_create(source: ExternRef, offset: u64, size: u64) -> (Module, SyscallResult);

    pub fn component_create() -> (Component, SyscallResult);

    pub fn component_add_instance(
        component: Component,
        module: Module,
    ) -> (SyscallResult, InstanceIndex);
}
