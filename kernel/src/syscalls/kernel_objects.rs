//! Kernel Objects
//!
//! Coral is an object-based kernel, in the sense that user-land interacts through the kernel via
//! handles to kernel-land objects. Kernel objects are reference counted.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::mem;

use crate::memory::VirtualMemoryArea;
use wasm::ExternRef64;

use spin::Mutex;

/// The currently active Virtual Memory Areas.
pub static ACTIVE_VMA: KernelObjectCollection<VirtualMemoryArea, VmaIndex> =
    KernelObjectCollection::new();

/// A collection of kernel objects.
pub struct KernelObjectCollection<Obj, Idx> {
    collection: Mutex<Vec<Arc<Obj>>>,
    _idx: PhantomData<Idx>,
}

/// Kernel Object Index.
///
/// A trait that represents a kernel object index, can be used to retrieve an object from a global
/// collection.
pub trait KoIndex {
    fn from(index: usize) -> Self;
    fn into_usize(self) -> usize;
    fn into_externref(self) -> ExternRef;
}

impl<Obj, Idx> KernelObjectCollection<Obj, Idx> {
    /// Creates an empty collection.
    const fn new() -> Self {
        Self {
            collection: Mutex::new(Vec::new()),
            _idx: PhantomData,
        }
    }
}

impl<Obj, Idx> KernelObjectCollection<Obj, Idx>
where
    Idx: KoIndex,
{
    /// Inserts a new object into the collection. The corresponding index is returned.
    pub fn insert(&self, object: Obj) -> Idx {
        let mut collection = self.collection.lock();
        let idx = collection.len();
        collection.push(Arc::new(object));
        Idx::from(idx)
    }

    /// Retrieves an object from the collection.
    pub fn get(&self, index: Idx) -> Option<Arc<Obj>> {
        let collection = self.collection.lock();
        collection.get(index.into_usize()).cloned()
    }
}

/// A WebAssembly externref.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ExternRef {
    /// A virtual memory area.
    Vma(VmaIndex),
}

/// This value is used to assert a compile time that ExternRef is 8 bytes long.
#[doc(hidden)]
const _EXTERNREF_SIZE_ASSERT: [u8; 8] = [0; mem::size_of::<ExternRef>()];

impl ExternRef64 for ExternRef {
    fn to_u64(self) -> u64 {
        // SAFETY: transmute check for the size at compile time, and because all 64 values are
        // valid u64 the result is always valid.
        //
        // TODO: can we do that without transmute?
        unsafe { mem::transmute(self) }
    }
}

/// An index representing a virtual memory area.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct VmaIndex(u32);

impl KoIndex for VmaIndex {
    fn from(index: usize) -> Self {
        let idx = u32::try_from(index).expect("Invalid VMA index");
        VmaIndex(idx)
    }

    fn into_usize(self) -> usize {
        self.0 as usize
    }

    fn into_externref(self) -> ExternRef {
        ExternRef::Vma(self)
    }
}
