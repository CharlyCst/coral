//! The Fallback Allocator
//!
//! The fallback allocator is used by other more specialized allocators when they run out of
//! memory. It is implemented as a linked list allocator with an O(n) allocation and deallocation.
//!
//! The fallback allocator is intended for the allocation of huge blocks that are subsequently
//! divided by specialized allocators, that way most of the allocations can be fast while still
//! being backed by a flexible general purpose allocator.

// TODO: Merge blocks on free.
// Instead of inserting free blocks at the begining of the free list, keep the list ordered and
// merge adjacent blocks.
// This makes performance much worse (O(n) free operations instead of O(1)) but is required to
// avoid fragmentation.

// TODO: Add some tests.
// The tests should first setup a heap spanning a few frames, and then perform the allocations.

use crate::allocator::utils::align_up;
use alloc::alloc::Layout;
use core::mem;
use core::ptr;

/// Nodes of the linked list.
///
/// The node structure is stored directly inside the free regions.
struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        Self { size, next: None }
    }

    fn start_addr(&self) -> usize {
        (self as *const Self) as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

pub struct FallbackAllocator {
    head: ListNode,
}

impl FallbackAllocator {
    /// Creates a new (uninitialized) allocator.
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
        }
    }

    /// Initializes the allocator with the given heap bounds.
    ///
    /// SAFETY: The caller must guarantee that the given heap bounds are valid and that the heap is
    /// unused. This method must be called only once.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.add_free_region(heap_start, heap_size)
    }

    /// Adds the given memory to the front of the list.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        // ensure that the freed region is capable of holding ListNode
        assert_eq!(align_up(addr, mem::align_of::<ListNode>()), addr);
        assert!(size >= mem::size_of::<ListNode>());

        // create a new list node and append it at the start of the list
        let mut node = ListNode::new(size);
        node.next = self.head.next.take();
        let node_ptr = addr as *mut ListNode;
        node_ptr.write(node);
        self.head.next = Some(&mut *node_ptr)
    }

    /// Looks for a free region with the given size and alignment and removes
    /// it from the list.
    ///
    /// Returns a tuple of the list node and the start address of the allocation.
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        // reference to current list node, updated for each iteration
        let mut current = &mut self.head;
        // look for a large enough memory region in linked list
        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(&region, size, align) {
                // region suitable for allocation -> remove node from list
                let next = region.next.take();
                let ret = Some((current.next.take().unwrap(), alloc_start));
                current.next = next;
                return ret;
            } else {
                // region not suitable -> continue with next region
                current = current.next.as_mut().unwrap();
            }
        }

        // no suitable region found
        None
    }

    /// Tries to use the given region for an allocation with given size and
    /// alignment.
    ///
    /// Returns the allocation start address on success.
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;

        if alloc_end > region.end_addr() {
            // region too small
            return Err(());
        }

        let excess_size = region.end_addr() - alloc_end;
        if excess_size > 0 && excess_size < mem::size_of::<ListNode>() {
            // rest of region too small to hold a ListNode (required because the
            // allocation splits the region in a used and a free part)
            return Err(());
        }

        // region suitable for allocation
        Ok(alloc_start)
    }

    /// Allocates a block with the given layout.
    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        // perform layout adjustments
        let (size, align) = Self::size_align(layout);

        if let Some((region, alloc_start)) = self.find_region(size, align) {
            let alloc_end = alloc_start.checked_add(size).expect("overflow");
            let excess_size = region.end_addr() - alloc_end;
            if excess_size > 0 {
                unsafe {
                    self.add_free_region(alloc_end, excess_size);
                }
            }
            alloc_start as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    /// Deallocates a block.
    ///
    /// SAFETY: The pointer must point to a block allocated by this allocator with the exact same
    /// layout as the layout passed to dealloc.
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        // perform layout adjustments
        let (size, _) = Self::size_align(layout);

        self.add_free_region(ptr as usize, size)
    }

    /// Adjust the given layout so that the resulting allocated memory
    /// region is also capable of storing a `ListNode`.
    ///
    /// Returns the adjusted size and alignment as a (size, align) tuple.
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("adjusting alignment failed")
            .pad_to_align();
        let size = layout.size().max(mem::size_of::<ListNode>());
        (size, layout.align())
    }
}
