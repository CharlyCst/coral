use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use core::marker::PhantomData;
use core::ptr::NonNull;

use wasm::{HeapKind, MemoryArea, ModuleError, RefType};

const PAGE_SIZE: usize = 0x1000;

// —————————————————————————————— Memory Area ——————————————————————————————— //

pub struct MMapArea {
    ptr: NonNull<u8>,
    size: usize,
    marker: PhantomData<u8>,
}

impl MMapArea {
    fn set_executable(&self) {
        // Special case for zero-sized allocations
        if self.size == 0 {
            return;
        }

        unsafe {
            let ok = libc::mprotect(
                self.ptr.as_ptr() as *mut libc::c_void,
                self.size,
                libc::PROT_READ | libc::PROT_EXEC,
            );
            if ok != 0 {
                panic!("Could not set memory executable",);
            }
        }
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.size) }
    }
}

impl MemoryArea for MMapArea {
    fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }
}

// ——————————————————————————————— Allocator ———————————————————————————————— //

pub struct LibcAllocator();

impl LibcAllocator {
    pub fn new() -> LibcAllocator {
        LibcAllocator()
    }
}

impl LibcAllocator {
    fn with_capacity(&self, n: usize) -> Result<MMapArea, ()> {
        let mut nb_pages = 1;
        while nb_pages * PAGE_SIZE < n {
            nb_pages += 1;
        }

        let size = PAGE_SIZE * nb_pages;
        let ptr = unsafe {
            libc::mmap(
                0 as *mut libc::c_void,
                PAGE_SIZE * nb_pages,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        } as *mut u8;

        if let Some(ptr) = NonNull::new(ptr) {
            Ok(MMapArea {
                ptr,
                size,
                marker: PhantomData,
            })
        } else {
            Err(())
        }
    }
}

// ——————————————————————————— Userspace Runtime ———————————————————————————— //

pub struct Runtime {
    alloc: LibcAllocator,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            alloc: LibcAllocator::new(),
        }
    }
}

unsafe impl wasm::Runtime for Runtime {
    type MemoryArea = Arc<MMapArea>;
    type Context = ();

    fn create_context(&self) -> Self::Context {}

    fn alloc_heap<F>(
        &self,
        min_size: usize,
        _kind: HeapKind,
        initialize: F,
        _ctx: &mut Self::Context,
    ) -> Result<Self::MemoryArea, ModuleError>
    where
        F: FnOnce(&mut [u8]) -> Result<(), ModuleError>,
    {
        let mut area = self
            .alloc
            .with_capacity(min_size as usize)
            .map_err(|_| wasm::ModuleError::RuntimeError)?;
        initialize(area.as_bytes_mut())?;
        Ok(Arc::new(area))
    }

    fn alloc_table(
        &self,
        min_size: u32,
        max_size: Option<u32>,
        _ty: RefType,
        _ctx: &mut Self::Context,
    ) -> Result<Box<[u64]>, ModuleError> {
        let size = if let Some(max_size) = max_size {
            max_size
        } else {
            min_size
        } as usize;
        Ok(vec![0; size].into_boxed_slice())
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
        let mut area = self
            .alloc
            .with_capacity(size)
            .map_err(|_| wasm::ModuleError::RuntimeError)?;
        write_code(area.as_bytes_mut())?;
        area.set_executable();
        Ok(Arc::new(area))
    }
}
