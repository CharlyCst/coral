#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(kernel::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use spin::Mutex;
use wasm::{MemoryAeaAllocator, MemoryArea};

use kernel;
use kernel::memory::VirtualMemoryAreaAllocator;

entry_point!(main);

static ALLOCATOR: Mutex<Option<VirtualMemoryAreaAllocator>> = Mutex::new(None);

fn main(boot_info: &'static BootInfo) -> ! {
    kernel::init();
    let allocator = unsafe { kernel::init_memory(boot_info).unwrap() };
    **&mut ALLOCATOR.lock() = Some(allocator);

    test_main();

    kernel::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::test_panic_handler(info)
}

#[test_case]
fn simple_allocation() {
    let heap_value_1 = Box::new(41);
    let heap_value_2 = Box::new(13);
    assert_eq!(*heap_value_1, 41);
    assert_eq!(*heap_value_2, 13);
}

#[test_case]
fn various_sizes() {
    for pow in 0..13 {
        let _ = Vec::<u8>::with_capacity(0x1 << pow);
    }
}

#[test_case]
fn alloc_vma() {
    let allocator = ALLOCATOR.lock();
    let allocator = allocator.as_ref().unwrap();
    let vma = allocator.with_capacity(0x1500).unwrap(); // one page and a half on x86

    // Try to fill the vma
    unsafe {
        for byte in vma.unsafe_as_bytes_mut() {
            *byte = 0;
        }
    }
}
