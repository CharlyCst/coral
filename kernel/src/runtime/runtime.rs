//! WebAssembly Runtime
//!
//! This module provides an implementation of `wasm::Runtime`, used for module instantation.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::memory::{Vma, VmaAllocator};
use crate::runtime::{VmaIndex, ACTIVE_VMA};
use crate::syscalls::ExternRef;
use wasm::{HeapKind, ModuleError, RefType, WasmType};

use super::KoIndex;

type Area = Arc<Vma>;

// ———————————————————————————— Runtime Context ————————————————————————————— //

/// A context passed to runtime methods during module instantiation.
pub struct InstantiationCtx {
    /// The owned heaps.
    heaps: Vec<VmaIndex>,
    /// The first externref table is filled with references to owned objects.
    is_first_externref_table: bool,
}

// ———————————————————————————————— Runtime ————————————————————————————————— //

/// The wasm runtime, responsible for allocating code and memory areas.
pub struct Runtime {
    alloc: VmaAllocator,
}

impl Runtime {
    pub fn new(alloc: VmaAllocator) -> Self {
        Self { alloc }
    }
}

unsafe impl wasm::Runtime for Runtime {
    type MemoryArea = Area;
    type Context = InstantiationCtx;

    fn create_context(&self) -> Self::Context {
        InstantiationCtx {
            heaps: Vec::new(),
            is_first_externref_table: true,
        }
    }

    fn alloc_heap<F>(
        &self,
        min_size: usize,
        _kind: HeapKind,
        initialize: F,
        ctx: &mut Self::Context,
    ) -> Result<Self::MemoryArea, ModuleError>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ModuleError>,
    {
        let mut vma = self
            .alloc
            .with_capacity(min_size as usize)
            .map_err(|_| ModuleError::FailedToInstantiate)?;
        initialize(vma.as_bytes_mut())?;
        let vma = Arc::new(vma);
        let vma_idx = ACTIVE_VMA.insert(Arc::clone(&vma));
        ctx.heaps.push(vma_idx);
        Ok(vma)
    }

    fn alloc_table(
        &self,
        min_size: u32,
        max_size: Option<u32>,
        ty: RefType,
        ctx: &mut Self::Context,
    ) -> Result<Box<[u64]>, ModuleError> {
        let size = if let Some(max_size) = max_size {
            max_size
        } else {
            min_size
        } as usize;
        let mut table = vec![ExternRef::Invalid.into_abi(); size].into_boxed_slice();

        if ctx.is_first_externref_table && ty == RefType::ExternRef {
            ctx.is_first_externref_table = false;

            // Fill the first table with heap references
            for (idx, vma) in ctx.heaps.iter().enumerate() {
                if idx >= table.len() {
                    break;
                }
                table[idx] = vma.into_externref().into_abi()
            }
        }

        Ok(table)
    }

    fn alloc_code<F>(
        &self,
        size: usize,
        write_code: F,
        _ctx: &mut Self::Context,
    ) -> Result<Self::MemoryArea, ModuleError>
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
