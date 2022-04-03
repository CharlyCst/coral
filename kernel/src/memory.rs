use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::frame::PhysFrame;
use x86_64::structures::paging::page_table::PageTable;
use x86_64::structures::paging::OffsetPageTable;
use x86_64::{PhysAddr, VirtAddr};

// ————————————————————————— Re-export definitions —————————————————————————— //

pub use x86_64::structures::paging::page::Size4KiB;
pub use x86_64::structures::paging::FrameAllocator;

pub trait Mapper: x86_64::structures::paging::Mapper<Size4KiB> {}
impl Mapper for OffsetPageTable<'static> {}

// —————————————————————————————— Page Tables ——————————————————————————————— //

/// Initialize a new OffsetPageTable.
pub unsafe fn init(physical_memory_offset: VirtAddr) -> impl Mapper + 'static {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
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
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
