#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(kernel::test_runner)]
#![reexport_test_harness_main = "test_main"]

use bootloader::{entry_point, BootInfo};
use core::arch::asm;
use core::panic::PanicInfo;
use wasm::{Compiler, Instance};

use compiler::X86_64Compiler;
use kernel::kprintln;

/// The first user program to run, expected to boostrap userspace.
const WASM_USERBOOT: &'static [u8; 169] = std::include_bytes!("../wasm/userboot.wasm");

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    kprintln!("Hello, {}!", "World");

    kernel::init();
    let allocator =
        unsafe { kernel::init_memory(boot_info).expect("Failed to initialize allocator") };

    // Initialize the Coral native module
    let coral_module = kernel::syscalls::build_syscall_module();
    let coral_instance = Instance::instantiate(&coral_module, Vec::new(), &allocator)
        .expect("Failed to instantiate coral syscalls");

    // Compile & initialize userboot
    let mut compiler = X86_64Compiler::new();
    compiler
        .parse(WASM_USERBOOT)
        .expect("Failed parsing userboot");
    let user_module = compiler.compile().expect("Failed compiling userboot");
    let userboot = Instance::instantiate(&user_module, vec![("coral", coral_instance)], &allocator)
        .expect("Failed to instantiate userboot");
    let userboot_init = userboot
        .get_func_addr_by_name("init")
        .expect("Failed to retrieve 'init' from userboot instance");
    let vmctx = userboot.get_vmctx_ptr();

    let result: u32;
    unsafe {
        asm!(
            "call {entry_point}",
            entry_point = in(reg) userboot_init,
            in("rdi") vmctx,
            out("rax") result,
        );
    }

    kprintln!("Userboot: {}", result);

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
