use core::marker::PhantomData;
use core::ptr::NonNull;

use ocean_wasm::{MemoryAeaAllocator, MemoryArea};

const PAGE_SIZE: usize = 0x1000;

// —————————————————————————————— Memory Area ——————————————————————————————— //

pub struct MMapArea {
    ptr: NonNull<u8>,
    size: usize,
    marker: PhantomData<u8>,
}

impl MemoryArea for MMapArea {
    fn set_executable(&mut self) {
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

    fn set_write(&mut self) {
        todo!()
    }

    fn set_read_only(&mut self) {
        todo!()
    }

    fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.size) }
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.size) }
    }

    fn size(&self) -> usize {
        self.size
    }

    fn extend_by(&mut self, _n: usize) -> Result<(), ()> {
        todo!()
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
