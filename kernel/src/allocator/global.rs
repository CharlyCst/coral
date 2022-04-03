//! # The Global Allocator
//!
//! The global allocator is a fized-size block allocators: it maintains lists of blocks of fixed
//! sizes for fast allocations and deallocations.
//!
//! For now the block sizes & alignments are power of 2, but in the future the allocator could be
//! tuned to accomodate the needs of the kernel.

use crate::allocator::FallbackAllocator;
use alloc::alloc::Layout;
use core::mem;

/// The block sizes to use.
///
/// The sizes must each be power of 2 because they are also used as the block alignment for now
/// (alignments must be always powers of 2).
macro_rules! block_sizes {
    ($($name:ident = $size:expr;)+) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        #[allow(non_camel_case_types)]
        pub enum BlockKind {
            $($name,)*
        }

        impl BlockKind {
            /// Returns the size of the block
            pub fn size(self) -> usize {
                match self {
                    $(BlockKind::$name => $size,)*
                }
            }

            /// Returns the alignment of the block.
            //
            //  NOTE: for now the aligment is equal to the size, we might want to change that in
            //  the future.
            pub fn align(self) -> usize {
                match self {
                    $(BlockKind::$name => $size,)*
                }
            }
        }

        struct Heads {
            $($name: Option<&'static mut ListNode>,)*
        }

        impl Heads {
            const fn empty() -> Self {
                Self {
                    $($name: None,)*
                }
            }

            /// Get the free list head for the corresponding layout, as well as the block kind
            /// (needed to allocate new blocks from the fallback allocator in case the free list is
            /// empty).
            /// Returns an error if no list can satisfy the request.
            fn get_head(&mut self, layout: Layout) -> Result<(&mut Option<&'static mut ListNode>, BlockKind), ()> {
                #![allow(non_upper_case_globals)]
                // the ranges are exclusives, so we have to add one
                $(
                    const $name: usize = $size +1;
                )*

                let required_block_size = layout.size().max(layout.align());
                match required_block_size {
                    $(0..$name => Ok((&mut self.$name, BlockKind::$name)),)*
                    _ => Err(())
                }
            }
        }
    }
}

block_sizes! {
    block_8    = 8;
    block_16   = 16;
    block_32   = 32;
    block_64   = 64;
    block_128  = 128;
    block_256  = 256;
    block_512  = 512;
    block_1024 = 1024;
    block_2048 = 2048;
}

/// A node of a free list.
struct ListNode {
    next: Option<&'static mut ListNode>,
}

pub struct GlobalAllocator {
    heads: Heads,
    fallback_allocator: FallbackAllocator,
}

impl GlobalAllocator {
    /// Creates a new (uninitialized) allocator.
    pub const fn new() -> Self {
        Self {
            heads: Heads::empty(),
            fallback_allocator: FallbackAllocator::new(),
        }
    }

    /// Initializes the allocator with the given heap bounds.
    ///
    /// SAFETY: The caller must guarantee that the given heap bounds are valid and that the heap is
    /// unused. This method must be called only once.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.fallback_allocator.init(heap_start, heap_size)
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        match self.heads.get_head(layout) {
            Ok((head, kind)) => {
                if let Some(node) = head.take() {
                    // There is a least one block avaiable
                    *head = node.next.take();
                    (node as *mut ListNode) as *mut u8
                } else {
                    // No block available, we need to allocate from the fallback allocator
                    // TODO: Allocate more than a single block here to amortize the cost of the
                    // fallback allocator.
                    let layout = Layout::from_size_align(kind.size(), kind.align()).unwrap();
                    self.fallback_allocator.alloc(layout)
                }
            }
            Err(_) => self.fallback_allocator.alloc(layout),
        }
    }

    /// Deallocates a block.
    ///
    /// SAFETY: The pointer must point to a block allocated by this allocator with the exact same
    /// layout as the layout passed to dealloc.
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        match self.heads.get_head(layout) {
            Ok((head, kind)) => {
                let new_node = ListNode { next: head.take() };
                // verify that block has size and alignment required for storing node
                assert!(mem::size_of::<ListNode>() <= kind.size());
                assert!(mem::align_of::<ListNode>() <= kind.align());
                let new_node_ptr = ptr as *mut ListNode;
                new_node_ptr.write(new_node);
                *head = Some(&mut *new_node_ptr);
            }
            Err(_) => self.fallback_allocator.dealloc(ptr, layout),
        }
    }
}
