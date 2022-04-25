#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(kernel::test_runner)]
#![reexport_test_harness_main = "test_main"]

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;

use kernel::kprintln;
use compiler::X86_64Compiler;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Print something and loop forever
    kprintln!("Hello, {}!", "World");

    kernel::init();
    let _allocator = unsafe { kernel::init_memory(boot_info) };

    let mut _compiler = X86_64Compiler::new();

    #[cfg(test)]
    test_main();

    kernel::hlt_loop();
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kprintln!("{}", info);

    kernel::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel::test_panic_handler(info);
}
