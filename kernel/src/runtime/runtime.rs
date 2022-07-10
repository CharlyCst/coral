//! WebAssembly Runtime
//!
//! This module provides an implementation of `wasm::Runtime`, used for module instantation.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::memory::{Vma, VmaAllocator};
use crate::syscalls::ExternRef;
use wasm::{ExternRef64, HeapKind, Instance, Module, ModuleError};

type Area = Arc<Vma>;

/// The wasm runtime, responsible for allocating code and memory areas.
pub struct Runtime {
    alloc: VmaAllocator,
}

impl Runtime {
    pub fn new(alloc: VmaAllocator) -> Self {
        Self { alloc }
    }

    pub fn instantiate(
        &self,
        module: &impl Module,
        import_from: Vec<(&str, Instance<Area>)>,
    ) -> Result<Instance<Area>, ModuleError> {
        Instance::instantiate(module, import_from, self)
    }
}

unsafe impl wasm::Runtime for Runtime {
    type MemoryArea = Area;

    fn alloc_heap(&self, min_size: u32, _kind: HeapKind) -> Result<Self::MemoryArea, ModuleError> {
        let mut vma = self
            .alloc
            .with_capacity(min_size as usize)
            .map_err(|_| ModuleError::FailedToInstantiate)?;
        vma.zeroed();
        Ok(Arc::new(vma))
    }

    fn alloc_table(&self, min_size: u32, max_size: Option<u32>) -> Result<Box<[u64]>, ModuleError> {
        let size = if let Some(max_size) = max_size {
            max_size
        } else {
            min_size
        } as usize;
        let table = vec![ExternRef::Invalid.to_u64(); size].into_boxed_slice();
        Ok(table)
    }

    fn alloc_code<F>(&self, size: usize, write_code: F) -> Result<Self::MemoryArea, ModuleError>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ModuleError>,
    {
        let mut vma = self
            .alloc
            .with_capacity(size)
            .map_err(|_| ModuleError::FailedToInstantiate)?;
        write_code(vma.as_bytes_mut())?;
        vma.set_executable();
        Ok(Arc::new(vma))
    }
}
