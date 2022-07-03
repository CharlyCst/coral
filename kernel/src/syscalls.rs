//! Coral System Calls
//!
//! System Calls in Coral are provided as a native module, that can be linked to any Wasm module.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::mem;

use crate::memory::VirtualMemoryArea;
use crate::{kprint, kprintln};
use wasm::{ExternRef64, NativeModule, NativeModuleBuilder, RawFuncPtr, MemoryArea};

use spin::Mutex;

// ————————————————————————————— Native Module —————————————————————————————— //

pub fn build_syscall_module(handles_table: Vec<ExternRef>) -> NativeModule {
    unsafe {
        NativeModuleBuilder::new()
            .add_func(
                String::from("print_char"),
                RawFuncPtr::new(print_char as *mut u8),
            )
            .add_func(
                String::from("buffer_write"),
                RawFuncPtr::new(vmo_write as *mut u8),
            )
            .add_table(String::from("handles"), handles_table)
            .build()
    }
}

// ————————————————————————————— Kernel Objects ————————————————————————————— //

pub static ACTIVE_VMA: KernelObjectCollection<VirtualMemoryArea, VmaIndex> =
    KernelObjectCollection::new();

pub struct KernelObjectCollection<Obj, Idx> {
    collection: Mutex<Vec<Arc<Obj>>>,
    _idx: PhantomData<Idx>,
}

pub trait KernelObjectIndex {
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
    Idx: KernelObjectIndex,
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

impl KernelObjectIndex for VmaIndex {
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

// —————————————————————————————— System Calls —————————————————————————————— //

/// The Virtual Machine Context, passed as argument to all instance functions, including native
/// functions.
type VmCtx = u64;

/// A WebAssembly u32.
type WasmU32 = u32;
/// A WebAssembly u64.
type WasmU64 = u64;

/// Prints a character.
///
/// The very first syscall! Useful for testing and debugging!
extern "sysv64" fn print_char(char: WasmU32, _vmctx: VmCtx) {
    if let Some(c) = char::from_u32(char) {
        kprint!("{}", c);
    }
}

extern "sysv64" fn vmo_write(
    handle: ExternRef,
    buffer: WasmU64,
    vma_offset: WasmU64,
    buffer_size: WasmU64,
    _vmctx: VmCtx,
) {
    kprintln!(
        "Buffer Write: {:?} - address 0x{:x} - offset 0x{:x} - len 0x{:x}",
        handle,
        buffer,
        vma_offset,
        buffer_size
    );
    let idx = match handle {
        ExternRef::Vma(vma_idx) => vma_idx,
    };
    let vma = match ACTIVE_VMA.get(idx) {
        Some(vma) => vma,
        None => return,
    };

    // SAFETY: TODO: what are the safety condition? Assume that userspace synchronized correctly?
    unsafe {
        let _buffer = vma.unsafe_as_bytes_mut();
        // TODO: get the instance memory
        // buffer[..vma_offset][..buffer_size]
    }
}
