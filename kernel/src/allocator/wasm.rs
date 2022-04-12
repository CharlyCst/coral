//! # Wasm Allocator
//!
//! This modules defines the allocators used for wasm module instances. Modules require memory for
//! different structures such as text section and memories, specialized allocators are provided to
//! allow for efficient allocation, expension and modification of permissions for these structures.

use alloc::alloc::{AllocError, Allocator, Layout};
use alloc::boxed::Box;
use core::ptr;

// ————————————————————————————— Code Allocator ————————————————————————————— //

/// The code allocator is responsible for allocating text sections of the wasm instances.
///
/// In order to enforce Write xor Execute accesses to text the pages are allocated in write mode
/// and must be explicitely set to execute mode after the code is written.
pub struct CodeAllocator {}

unsafe impl Allocator for CodeAllocator {
    fn allocate(&self, layout: Layout) -> Result<ptr::NonNull<[u8]>, AllocError> {
        todo!()
    }

    unsafe fn deallocate(&self, ptr: ptr::NonNull<u8>, layout: Layout) {
        todo!()
    }
}

// ————————————————————————————— Heap Allocator ————————————————————————————— //

/// The heap allocator is responsible for allocating pages for wasm instances' memories.
pub struct HeapAllocator {}

unsafe impl Allocator for HeapAllocator {
    fn allocate(&self, layout: Layout) -> Result<ptr::NonNull<[u8]>, AllocError> {
        todo!()
    }

    unsafe fn deallocate(&self, ptr: ptr::NonNull<u8>, layout: Layout) {
        todo!()
    }
}

// ————————————————————————————— Wasm Allocator ————————————————————————————— //

pub struct WasmAllocator {}

impl wasm::Allocator for WasmAllocator {
    type CodeAllocator = CodeAllocator;
    type HeapAllocator = HeapAllocator;

    fn alloc_code(&self, code_size: u32) -> Box<[u8], Self::CodeAllocator> {
        todo!()
    }

    fn set_executable(&self, ptr: &Box<[u8], Self::CodeAllocator>) {
        todo!()
    }

    fn alloc_heap(&self, min_size: u32, kind: wasm::HeapKind) -> Box<[u8], Self::HeapAllocator> {
        todo!()
    }
}
