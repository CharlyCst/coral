use crate::traits::{FuncIndex, GlobInit, HeapIndex, ImportIndex};
use crate::traits::{GlobIndex, VMContextLayout};
use collections::EntityRef;

use alloc::alloc::{alloc, dealloc, Layout};
use core::ptr::NonNull;

/// Size of a pointer, in bytes.
const PTR_SIZE: usize = core::mem::size_of::<*const u8>();
/// 8 bytes aligment.
const ALIGN_8: usize = core::mem::align_of::<u64>();
/// The width of items in the VMContext.
const ITEM_WIDTH: usize = 8;

pub struct VMContext {
    ptr: NonNull<u8>,
    layout: Layout,
    func_offset: usize,
    import_offset: usize,
    glob_offset: usize,
}

impl VMContext {
    /// Initialize an empty VMContext.
    ///
    /// WARNING: The VMContext **must** be initialized (with the various methods to set its field)
    /// before being used to execute any code. Failing to do so will result in undefined behavior.
    pub fn empty(layout: &impl VMContextLayout) -> Self {
        // For now each slot takes 8 bytes, in the future we will have to support other sizes (e.g.
        // for 128 bits globals), but this should be good enough to start with.
        let func_offset = layout.heaps().len() * ITEM_WIDTH;
        let import_offset = func_offset + layout.funcs().len() * ITEM_WIDTH;
        let glob_offset = import_offset + layout.imports().len() * ITEM_WIDTH;
        let capacity = glob_offset + layout.globs().len() * ITEM_WIDTH;

        let alloc_layout = Layout::from_size_align(capacity, ALIGN_8).unwrap();
        let ptr = unsafe { alloc(alloc_layout) };
        let ptr = NonNull::new(ptr).unwrap(); // TODO: handle allocation errors

        Self {
            ptr,
            layout: alloc_layout,
            func_offset,
            import_offset,
            glob_offset,
        }
    }

    pub fn set_heap(&mut self, heap_ptr: *const u8, idx: HeapIndex) {
        unsafe {
            let offset = idx.index() * PTR_SIZE;
            self.wirte_ptr_at(heap_ptr, offset);
        }
    }

    pub fn set_func(&mut self, func_ptr: *const u8, idx: FuncIndex) {
        unsafe {
            let offset = self.func_offset + idx.index() * PTR_SIZE;
            self.wirte_ptr_at(func_ptr, offset);
        }
    }

    pub fn set_import(&mut self, vmctx_ptr: *const u8, idx: ImportIndex) {
        unsafe {
            let offset = self.import_offset + idx.index() * PTR_SIZE;
            self.wirte_ptr_at(vmctx_ptr, offset);
        }
    }

    pub fn set_glob_ptr(&mut self, glob_ptr: *const u8, idx: GlobIndex) {
        unsafe {
            let offset = self.glob_offset + idx.index() * PTR_SIZE;
            self.wirte_ptr_at(glob_ptr, offset);
        }
    }

    pub fn set_glob_value(&mut self, value: GlobInit, idx: GlobIndex) {
        unsafe {
            let offset = self.glob_offset + idx.index() * PTR_SIZE;
            let ptr = self.ptr.as_ptr().add(offset);
            match value {
                GlobInit::I32(x) => ptr.cast::<i32>().write(x),
                GlobInit::I64(x) => ptr.cast::<i64>().write(x),
                GlobInit::F32(x) => ptr.cast::<u32>().write(x),
                GlobInit::F64(x) => ptr.cast::<u64>().write(x),
            }
        }
    }

    pub fn get_global_ptr(&self, idx: GlobIndex) -> *const u8 {
        unsafe {
            let offset = self.glob_offset + idx.index() * PTR_SIZE;
            self.ptr.as_ptr().add(offset)
        }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    unsafe fn wirte_ptr_at(&mut self, ptr: *const u8, offset: usize) {
        let target = self.ptr.as_ptr().add(offset).cast::<*const u8>();
        target.write(ptr);
    }
}

impl Drop for VMContext {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}
