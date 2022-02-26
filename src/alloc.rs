use crate::traits;

// ———————————————————————————— Module Allocator ———————————————————————————— //

const PAGE_SIZE: usize = 0x1000;

pub struct LibcCodeAllocator();

unsafe impl core::alloc::Allocator for LibcCodeAllocator {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        todo!()
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        // We just leak the memory
    }
}

impl traits::WriteXorExec for LibcCodeAllocator {
    fn set_execute(&self, ptr: Box<[u8]>) {
        let size = ptr.len();
        let mut nb_pages = 1;
        while nb_pages * PAGE_SIZE < size {
            nb_pages += 1;
        }

        unsafe {
            let ok = libc::mprotect(
                ptr.as_ptr() as *mut libc::c_void,
                nb_pages * PAGE_SIZE,
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
}

pub struct LibcHeapAllocator();

unsafe impl core::alloc::Allocator for LibcHeapAllocator {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, std::alloc::AllocError> {
        todo!()
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        // We just leak the memory
    }
}

pub struct LibcAllocator {}

impl LibcAllocator {
    pub fn new() -> Self {
        Self {}
    }

    fn alloc(&self, nb_pages: usize) -> *mut u8 {
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
        if ptr.is_null() {
            panic!("Failled mmap for heap allocation");
        }

        ptr
    }
}

impl traits::Allocator for LibcAllocator {
    type CodeAllocator = LibcCodeAllocator;
    type HeapAllocator = LibcHeapAllocator;

    fn alloc_code(&self, code_size: u32) -> Box<[u8], Self::CodeAllocator> {
        let code_size = code_size as usize;
        let mut nb_pages = 1;
        while nb_pages * PAGE_SIZE < code_size as usize {
            nb_pages += 1;
        }
        let ptr = self.alloc(nb_pages);

        unsafe {
            Box::from_raw_in(
                std::slice::from_raw_parts_mut(ptr, code_size),
                LibcCodeAllocator(),
            )
        }
    }

    fn alloc_heap(
        &self,
        min_size: u32,
        _max_size: Option<u32>,
        _kind: traits::HeapKind,
    ) -> Box<[u8], Self::HeapAllocator> {
        let ptr = self.alloc(min_size as usize);

        unsafe {
            Box::from_raw_in(
                std::slice::from_raw_parts_mut(ptr, min_size as usize),
                LibcHeapAllocator(),
            )
        }
    }
}


