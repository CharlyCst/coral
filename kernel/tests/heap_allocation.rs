#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(kernel::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use alloc::boxed::Box;
use alloc::vec::Vec;
use kernel;

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    kernel::init();
    unsafe { kernel::init_memory(boot_info) };

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
