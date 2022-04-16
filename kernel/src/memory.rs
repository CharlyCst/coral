use alloc::sync::Arc;
use core::marker::PhantomData;
use core::ops::DerefMut;
use core::ptr::NonNull;

use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use bootloader::BootInfo;
use spin::{Mutex, MutexGuard};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page::Page;
use x86_64::structures::paging::page_table::{PageTable, PageTableFlags};
use x86_64::structures::paging::{Mapper, OffsetPageTable};
use x86_64::{PhysAddr, VirtAddr};

use crate::allocator;
use wasm::{MemoryAeaAllocator, MemoryArea};

// TODO: Be generic over page sizes.
const PAGE_SIZE: usize = 0x1000;
const NB_PTE_ENTRIES: usize = 512;

// ————————————————————————— Re-export definitions —————————————————————————— //

pub use x86_64::structures::paging::page::Size4KiB;

pub trait FrameAllocator: x86_64::structures::paging::FrameAllocator<Size4KiB> {}

impl FrameAllocator for BootInfoFrameAllocator {}

// ————————————————————————— Memory Initialization —————————————————————————— //

/// Initializes the memory subsystem.
///
/// After success, the memory subsystem is operationnal, meaning that the global allocator is
/// availables (and thus heap allocated values such as `Box` and `Vec` can be used).
///
/// SAFETY: This function must be called **at most once**, and the boot info must contain a valid
/// mapping of the physical memory.
pub unsafe fn init(boot_info: &'static BootInfo) -> Result<VirtualMemoryAreaAllocator, ()> {
    let physical_memory_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let level_4_table = active_level_4_table(physical_memory_offset);

    // Initialize the frame allocator and the memory mapper.
    let mut frame_allocator = BootInfoFrameAllocator::init(&boot_info.memory_map);
    let mut mapper = OffsetPageTable::new(level_4_table, physical_memory_offset);

    // Initialize the heap.
    allocator::init_heap(&mut mapper, &mut frame_allocator).map_err(|_| ())?;

    // Create a memory map once the heap has been allocated.
    let memory_map = VirtualMemoryMap::new_from_mapping(mapper.level_4_table());

    Ok(VirtualMemoryAreaAllocator::new(
        mapper,
        memory_map,
        frame_allocator,
    ))
}

/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must be only called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table
}

// ———————————————————————————— Frame Allocator ————————————————————————————— //
// NOTE: this implementation comes from [1], it is simple but don't allow     //
// frame reuse and has an allocation inf O(n) where n is the number of        //
// already allocated framed.                                                  //
//                                                                            //
// [1]: https://os.phil-opp.com/paging-implementation/                        //
// —————————————————————————————————————————————————————————————————————————— //

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    /// Returns an iterator over the usable frames specified in the memory map.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // get usable regions from memory map
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        // map each region to its address range
        let addr_ranges = usable_regions.map(|r| r.range.start_addr()..r.range.end_addr());
        // transform to an iterator of frame start addresses
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // create `PhysFrame` types from the start addresses
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }

    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        Self::allocate_frame(self)
    }
}

// ——————————————————————————— Virtual Memory Map ——————————————————————————— //

/// A map of used & available chunks of an address space.
///
/// TODO: For now the memory map does not enable virtual addresses re-use. This was done for
/// simplicity of the initial implementation.
pub struct VirtualMemoryMap {
    // Next available address.
    cursor: VirtAddr,

    // End of the valid virtual address range.
    end_at: VirtAddr,
}

impl VirtualMemoryMap {
    /// Creates a mapping of the virtual memory map from the page tables.
    ///
    /// SAFETY: the page table must be a valid level 4 page table.
    pub unsafe fn new_from_mapping(level_4_table: &PageTable) -> Self {
        let (last_used_index, _) = level_4_table
            .iter()
            .enumerate()
            .filter(|(_idx, entry)| !entry.is_unused())
            .last()
            .unwrap();

        if last_used_index >= NB_PTE_ENTRIES {
            // Return a map with no free aeas
            VirtualMemoryMap {
                cursor: VirtAddr::new(0),
                end_at: VirtAddr::new(0),
            }
        } else {
            let l4_shift = 9 + 9 + 9 + 12; // Shift to get virtual address from L4 index
            let first_unused_index = (last_used_index + 1) as u64;
            let last_available_index = (NB_PTE_ENTRIES - 1) as u64;
            let cursor = VirtAddr::new(first_unused_index << l4_shift);
            let end_at = VirtAddr::new(last_available_index << l4_shift);
            crate::println!("Memory Map: {:x} -> {:x}", cursor.as_u64(), end_at.as_u64());
            VirtualMemoryMap { cursor, end_at }
        }
    }

    /// Reserves an area in the virtual address space.
    ///
    /// No frames are allocated, but the area is marked as reserved, preventing future collisions
    /// with other areas.
    pub fn reserve_area(&mut self, size: usize) -> Result<VirtAddr, ()> {
        let start_of_area = self.cursor;
        let end_of_area = (start_of_area + size).align_up(PAGE_SIZE as u64);
        if end_of_area > self.end_at {
            return Err(());
        }
        self.cursor = end_of_area;
        Ok(start_of_area)
    }
}

// —————————————————————————— Virtual Memory Area ——————————————————————————— //

// TODO: Free the area on drop.
pub struct VirtualMemoryArea {
    ptr: NonNull<u8>,
    nb_pages: usize,
    vma_allocator: VirtualMemoryAreaAllocator,
    marker: PhantomData<u8>,
}

impl VirtualMemoryArea {
    /// Returns the number of pages to add in order to grow by at least `n` bytes.
    fn bytes_to_pages(n: usize) -> usize {
        let page_aligned = (n + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        page_aligned / PAGE_SIZE
    }

    /// Set the given flags for all pages of the virtual memory area.
    ///
    /// WARNING: future accesses to the VMA might cause an exception if the appropriate flags are
    /// not present.
    fn update_flags(&mut self, flags: PageTableFlags) -> Result<(), ()> {
        let mut virt_addr = VirtAddr::from_ptr(self.ptr.as_ptr());
        let mut allocator = self.vma_allocator.lock();
        let mapper = &mut allocator.mapper;

        // The assumption is not necessary for correctness here, but should still hold.
        debug_assert!(virt_addr.is_aligned(PAGE_SIZE as u64));

        for _ in 0..self.nb_pages {
            let page = Page::<Size4KiB>::containing_address(virt_addr);
            unsafe {
                mapper.update_flags(page, flags).map_err(|_| ())?.flush();
            }
            virt_addr += PAGE_SIZE;
        }

        Ok(())
    }
}

impl MemoryArea for VirtualMemoryArea {
    fn set_executable(&mut self) {
        let flags = PageTableFlags::PRESENT;
        self.update_flags(flags)
            .expect("Could not set execute permission");
    }

    fn set_write(&mut self) {
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        self.update_flags(flags)
            .expect("Could not set write permission");
    }

    fn set_read_only(&mut self) {
        let flags = PageTableFlags::PRESENT | PageTableFlags::NO_EXECUTE;
        self.update_flags(flags)
            .expect("Could not set read-only permission");
    }

    fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    fn as_bytes(&self) -> &[u8] {
        // SAFETY: We rely on the correctness of `self.size()` and the validity of the pointer.
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.size()) }
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: We rely on the correctness of `self.size()` and the validity of the pointer.
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.size()) }
    }

    fn size(&self) -> usize {
        self.nb_pages * PAGE_SIZE
    }

    fn extend_by(&mut self, _n: usize) -> Result<(), ()> {
        todo!()
    }
}

/// The Virtual Memory Area Allocator, responsible for allocating and managing virtual memory
/// areas.
pub struct VirtualMemoryAreaAllocator(Arc<Mutex<LockedVirtualMemoryAreaAllocator>>);

impl VirtualMemoryAreaAllocator {
    fn lock(&self) -> MutexGuard<LockedVirtualMemoryAreaAllocator> {
        self.0.lock()
    }
}

impl Clone for VirtualMemoryAreaAllocator {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Internal state of the `VirtualMemoryAreaAllocator`.
struct LockedVirtualMemoryAreaAllocator {
    mapper: OffsetPageTable<'static>,
    memory_map: VirtualMemoryMap,
    frame_allocator: BootInfoFrameAllocator,
}

impl VirtualMemoryAreaAllocator {
    pub fn new(
        mapper: OffsetPageTable<'static>,
        memory_map: VirtualMemoryMap,
        frame_allocator: BootInfoFrameAllocator,
    ) -> Self {
        let inner = Arc::new(Mutex::new(LockedVirtualMemoryAreaAllocator {
            mapper,
            memory_map,
            frame_allocator,
        }));
        Self(inner)
    }
}

impl MemoryAeaAllocator for VirtualMemoryAreaAllocator {
    type Area = VirtualMemoryArea;

    // TODO: Free allocated pages on failure.
    fn with_capacity(&self, capacity: usize) -> Result<Self::Area, ()> {
        let nb_pages = VirtualMemoryArea::bytes_to_pages(capacity);
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        let mut inner = self.0.lock();
        let inner = inner.deref_mut();
        let mapper = &mut inner.mapper;
        let frame_allocator = &mut inner.frame_allocator;
        let mut virt_addr = inner.memory_map.reserve_area(capacity)?;
        let ptr = NonNull::new(virt_addr.as_mut_ptr()).unwrap();

        for _ in 0..nb_pages {
            unsafe {
                let frame = frame_allocator.allocate_frame().ok_or(())?;
                let page = Page::containing_address(virt_addr);
                mapper
                    .map_to(page, frame, flags, frame_allocator)
                    .map_err(|_| ())?
                    .flush();
                virt_addr += PAGE_SIZE;
            }
        }

        Ok(VirtualMemoryArea {
            ptr,
            nb_pages,
            vma_allocator: self.clone(),
            marker: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type VMA = VirtualMemoryArea;

    #[test_case]
    fn bytes_to_pages() {
        assert_eq!(VMA::bytes_to_pages(0), 0);
        assert_eq!(VMA::bytes_to_pages(1), 1);
        assert_eq!(VMA::bytes_to_pages(PAGE_SIZE - 1), 1);
        assert_eq!(VMA::bytes_to_pages(PAGE_SIZE), 1);
        assert_eq!(VMA::bytes_to_pages(PAGE_SIZE + 1), 2);
    }
}
