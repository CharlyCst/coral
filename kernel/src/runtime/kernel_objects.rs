//! Kernel Objects
//!
//! Coral is an object-based kernel, in the sense that user-land interacts through the kernel via
//! handles to kernel-land objects. Kernel objects are reference counted.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::marker::PhantomData;

use crate::memory::Vma;
use crate::syscalls::ExternRef;
use crate::wasm::Component;
use wasm::WasmModule;

use spin::Mutex;

/// The currently active Virtual Memory Areas.
pub static ACTIVE_VMA: KernelObjectCollection<Vma, VmaIndex> = KernelObjectCollection::new();

/// The currently active WebAssembly modules.
pub static ACTIVE_MODULES: KernelObjectCollection<WasmModule, ModuleIndex> =
    KernelObjectCollection::new();

/// The currently active components.
pub static ACTIVE_COMPONENTS: KernelObjectCollection<Component, ComponentIndex> =
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
    pub fn insert(&self, object: Arc<Obj>) -> Idx {
        let mut collection = self.collection.lock();
        let idx = collection.len();
        collection.push(object);
        Idx::from(idx)
    }

    /// Retrieves an object from the collection.
    pub fn get(&self, index: Idx) -> Option<Arc<Obj>> {
        let collection = self.collection.lock();
        collection.get(index.into_usize()).cloned()
    }
}

/// An index representing a virtual memory area.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct VmaIndex(u32);

/// An index representing a WebAssembly module.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ModuleIndex(u32);

/// An index representing a component.
#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ComponentIndex(u32);

macro_rules! impl_ko_index {
    ($index:ident, $handle:tt, $error:expr) => {
        impl KoIndex for $index {
            fn from(index: usize) -> Self {
                let idx = u32::try_from(index).expect($error);
                Self(idx)
            }

            fn into_usize(self) -> usize {
                self.0 as usize
            }

            fn into_externref(self) -> ExternRef {
                ExternRef::$handle(self)
            }
        }
    };
}

impl_ko_index!(VmaIndex, Vma, "Invalid VMA index");
impl_ko_index!(ModuleIndex, Module, "Invalid module index");
impl_ko_index!(ComponentIndex, Component, "Invalid component index");
