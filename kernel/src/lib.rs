#![no_std]
// Some unstable features, to remove once stabilized.
#![feature(exclusive_range_pattern)]
#![feature(custom_test_frameworks)]
#![feature(alloc_error_handler)]
#![feature(abi_x86_interrupt)]
#![feature(const_mut_refs)]
#![feature(allocator_api)]
// Setup test config
#![cfg_attr(test, no_main)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

#[cfg(test)]
use bootloader::entry_point;
#[cfg(test)]
use bootloader::BootInfo;
use core::panic::PanicInfo;

pub mod allocator;
pub mod gdt;
pub mod interrupts;
pub mod memory;
pub mod qemu;
pub mod serial;
pub mod vga;

pub use memory::init as init_memory;

#[cfg(test)]
entry_point!(test_kernel_main);

// Entry point for `cargo test`
#[cfg(test)]
fn test_kernel_main(_boot_info: &'static BootInfo) -> ! {
    init();
    test_main();

    hlt_loop();
}

/// Initializes the kernel environment.
pub fn init() {
    // Initialize description tables
    gdt::init();
    interrupts::init_idt();

    // Initialize hardware interrupt
    unsafe { interrupts::PICS.lock().initialize() };
    x86_64::instructions::interrupts::enable();
}

/// An infinite loop that causes the CPU to halt between interrupts.
pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}

// —————————————————————————————————— Test —————————————————————————————————— //

pub trait Testable {
    fn run(&self) -> ();
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

/// A custom test runner for the kernel.
pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }

    qemu::exit(qemu::ExitCode::Success);
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info);
}

/// A custom panic handler for kernel testing.
pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    qemu::exit(qemu::ExitCode::Failed);
    hlt_loop();
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn test() {
        assert_eq!(1, 1);
    }

    #[test_case]
    fn breakpoint_exception() {
        x86_64::instructions::interrupts::int3();
    }
}
