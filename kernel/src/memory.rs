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
use wasm::MemoryArea;

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
pub unsafe fn init(boot_info: &'static BootInfo) -> Result<VmaAllocator, ()> {
    let physical_memory_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let level_4_table = active_level_4_table(physical_memory_offset);

    // Initialize the frame allocator and the memory mapper.
    let mut frame_allocator = BootInfoFrameAllocator::init(&boot_info.memory_map);
    let mut mapper = OffsetPageTable::new(level_4_table, physical_memory_offset);

    // Initialize the heap.
    allocator::init_heap(&mut mapper, &mut frame_allocator).map_err(|_| ())?;

    // Create a memory map once the heap has been allocated.
    let memory_map = VirtualMemoryMap::new_from_mapping(mapper.level_4_table());

    Ok(VmaAllocator::new(
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

/// A Virtual Memory Area.
// TODO: Free the area on drop.
pub struct Vma {
    ptr: NonNull<u8>,
    nb_pages: usize,
    size: usize,
    #[allow(unused)]
    kind: VmaKind,
    vma_allocator: Option<VmaAllocator>,
    marker: PhantomData<u8>,
}

pub enum VmaKind {
    /// Satic VMA, for instance a VGA buffer or fixed-size area.
    Static,
}

// SAFETY: VMA's operation are thread safe, except writting to the area which must be properly
// synchronized by the caller (e.g. a Wasm instance).
unsafe impl Send for Vma {}
unsafe impl Sync for Vma {}

impl Vma {
    /// Returns the number of pages to add in order to grow by at least `n` bytes.
    fn bytes_to_pages(n: usize) -> usize {
        let page_aligned = (n + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        page_aligned / PAGE_SIZE
    }

    /// Set the given flags for all pages of the virtual memory area.
    ///
    /// WARNING: future accesses to the VMA might cause an exception if the appropriate flags are
    /// not present.
    fn update_flags(&self, flags: PageTableFlags) -> Result<(), ()> {
        let mut virt_addr = VirtAddr::from_ptr(self.ptr.as_ptr());
        let mut allocator = self.vma_allocator.as_ref().ok_or(())?.lock();
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

    /// Fill the area with zero.
    pub fn zeroed(&mut self) {
        let area = self.as_bytes_mut();
        area.fill(0);
    }

    /// Builds a virtual memory from a raw pointer.
    ///
    /// SAFETY: The corresponding memory area must be valid for the whole existance of the VMA.
    /// The VMA should then be considered the owner of that area.
    pub unsafe fn from_raw(ptr: NonNull<u8>, size: usize) -> Self {
        let nb_pages = Self::bytes_to_pages(size);
        Self {
            ptr,
            nb_pages,
            size,
            kind: VmaKind::Static,
            vma_allocator: None,
            marker: PhantomData,
        }
    }

    /// Sets the area executable.
    ///
    /// Removes write permission.
    pub fn set_executable(&self) {
        let flags = PageTableFlags::PRESENT;
        self.update_flags(flags)
            .expect("Could not set execute permission");
    }

    /// Sets area writeable.
    ///
    /// Removes execute permission.
    pub fn set_write(&self) {
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
        self.update_flags(flags)
            .expect("Could not set write permission");
    }

    /// Sets the area as read-only.
    ///
    /// Removes both write and execute permissions.
    pub fn set_read_only(&self) {
        let flags = PageTableFlags::PRESENT | PageTableFlags::NO_EXECUTE;
        self.update_flags(flags)
            .expect("Could not set read-only permission");
    }

    /// Returns a view of the area.
    ///
    /// SAFETY: The area might be subject to mutation through internal mutabilities (e.g. if the
    /// area serves as an instance heap). Therefore read accesses must be properly synchronized.
    ///
    /// TODO: Mark as unsafe
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: We rely on the correctness of `self.size()` and the validity of the pointer.
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.size()) }
    }

    /// Returns a mutable view of the area.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: we got a mutable reference to the area, so it's safe to return a mutable slice
        // of the area as we can't have any other references (therefore no risks of concurrent
        // updates).
        unsafe { self.unsafe_as_bytes_mut() }
    }

    /// Returns a view of the area.
    ///
    /// SAFETY: The area might be subject to mutation through internal mutabilities (e.g. if the
    /// area serves as an instance heap). Therefore read accesses must be properly synchronized.
    pub unsafe fn unsafe_as_bytes_mut(&self) -> &mut [u8] {
        // SAFETY: We rely on the correctness of `self.size()` and the validity of the pointer.
        // The caller is responsible for ensuring that there is no alisaing &mut references.
        core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.size())
    }

    /// Returns the size of the slice.
    pub fn size(&self) -> usize {
        self.size
    }
}

impl MemoryArea for Vma {
    fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }
}

// ————————————————————— Virtual Memory Area Allocator —————————————————————— //

/// The Virtual Memory Area Allocator, responsible for allocating and managing virtual memory
/// areas.
pub struct VmaAllocator(Arc<Mutex<LockedVmaAllocator>>);

impl VmaAllocator {
    fn lock(&self) -> MutexGuard<LockedVmaAllocator> {
        self.0.lock()
    }
}

impl Clone for VmaAllocator {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Internal state of the `VirtualMemoryAreaAllocator`.
struct LockedVmaAllocator {
    mapper: OffsetPageTable<'static>,
    memory_map: VirtualMemoryMap,
    frame_allocator: BootInfoFrameAllocator,
}

impl VmaAllocator {
    pub fn new(
        mapper: OffsetPageTable<'static>,
        memory_map: VirtualMemoryMap,
        frame_allocator: BootInfoFrameAllocator,
    ) -> Self {
        let inner = Arc::new(Mutex::new(LockedVmaAllocator {
            mapper,
            memory_map,
            frame_allocator,
        }));
        Self(inner)
    }

    /// Allocates a new virtual memory area with the given capacity.
    // TODO: Free allocated pages on failure.
    pub fn with_capacity(&self, capacity: usize) -> Result<Vma, ()> {
        let nb_pages = Vma::bytes_to_pages(capacity);
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

        Ok(Vma {
            ptr,
            nb_pages,
            size: capacity,
            kind: VmaKind::Static, // TODO: We don't support resizing for now.
            vma_allocator: Some(self.clone()),
            marker: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type VMA = Vma;

    #[test_case]
    fn bytes_to_pages() {
        assert_eq!(VMA::bytes_to_pages(0), 0);
        assert_eq!(VMA::bytes_to_pages(1), 1);
        assert_eq!(VMA::bytes_to_pages(PAGE_SIZE - 1), 1);
        assert_eq!(VMA::bytes_to_pages(PAGE_SIZE), 1);
        assert_eq!(VMA::bytes_to_pages(PAGE_SIZE + 1), 2);
    }
}
