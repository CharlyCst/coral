//! # Heap Allocator

use x86_64::structures::paging::mapper::{MapToError, Mapper};
use x86_64::structures::paging::{Page, PageTableFlags};
use x86_64::VirtAddr;

use crate::memory::{FrameAllocator, Size4KiB};
use alloc::alloc::GlobalAlloc;
use core::sync::atomic::{AtomicBool, Ordering};

mod fallback;
mod global;
mod utils;

pub use fallback::FallbackAllocator;

pub const HEAP_START: usize = 0x4444_4444_0000;
pub const HEAP_SIZE: usize = 20 * 0x1000;

static IS_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initializes the kernel heap.
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator,
) -> Result<(), MapToError<Size4KiB>> {
    if IS_INITIALIZED.swap(true, Ordering::SeqCst) {
        // Already initialized
        return Ok(());
    }

    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() };
    }

    // SAFETY: We check that the method is called only once and the heap is valid (mappings are
    // created just above).
    unsafe {
        GLOBAL_ALLOC.lock().init(HEAP_START, HEAP_SIZE);
    }

    Ok(())
}

// —————————————————————————— The Global Allocator —————————————————————————— //

#[global_allocator]
static GLOBAL_ALLOC: utils::Locked<global::GlobalAllocator> =
    utils::Locked::new(global::GlobalAllocator::new());

unsafe impl GlobalAlloc for utils::Locked<global::GlobalAllocator> {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.lock().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        self.lock().dealloc(ptr, layout)
    }
}
