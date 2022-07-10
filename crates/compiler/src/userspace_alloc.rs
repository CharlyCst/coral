use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use core::marker::PhantomData;
use core::ptr::NonNull;

use wasm::{ExclusiveMemoryArea, HeapKind, MemoryAeaAllocator, MemoryArea, ModuleError};

const PAGE_SIZE: usize = 0x1000;

// —————————————————————————————— Memory Area ——————————————————————————————— //

pub struct MMapArea {
    ptr: NonNull<u8>,
    size: usize,
    marker: PhantomData<u8>,
}

impl MemoryArea for MMapArea {
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
                panic!(
                    "Could not set memory executable: errno {}",
                    *libc::__errno_location()
                );
            }
        }
    }

    fn set_write(&self) {
        todo!()
    }

    fn set_read_only(&self) {
        todo!()
    }

    fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.size) }
    }

    unsafe fn unsafe_as_bytes_mut(&self) -> &mut [u8] {
        core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.size)
    }

    fn size(&self) -> usize {
        self.size
    }

    fn extend_by(&self, _n: usize) -> Result<(), ()> {
        todo!()
    }
}

impl ExclusiveMemoryArea for MMapArea {
    type Shared = Arc<MMapArea>;

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        // Safety: we got a mutable reference, so returning a mutable slice of the area is
        // perfectly fine.
        unsafe { self.unsafe_as_bytes_mut() }
    }

    fn into_shared(self) -> Self::Shared {
        Arc::new(self)
    }
}

// ——————————————————————————————— Allocator ———————————————————————————————— //

pub struct LibcAllocator();

impl LibcAllocator {
    pub fn new() -> LibcAllocator {
        LibcAllocator()
    }
}

impl MemoryAeaAllocator for LibcAllocator {
    type Area = MMapArea;

    fn with_capacity(&self, n: usize) -> Result<Self::Area, ()> {
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

    fn alloc_heap(&self, min_size: u32, _kind: HeapKind) -> Result<Self::MemoryArea, ModuleError> {
        let area = self
            .alloc
            .with_capacity(min_size as usize)
            .map_err(|_| wasm::ModuleError::RuntimeError)?;
        Ok(Arc::new(area))
    }

    fn alloc_table(&self, min_size: u32, max_size: Option<u32>) -> Result<Box<[u64]>, ModuleError> {
        let size = if let Some(max_size) = max_size {
            max_size
        } else {
            min_size
        } as usize;
        Ok(vec![0; size].into_boxed_slice())
    }

    fn alloc_code<F>(&self, size: usize, write_code: F) -> Result<Self::MemoryArea, ModuleError>
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
