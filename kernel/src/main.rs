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

const WASM_USERBOOT: &'static [u8; 37] = std::include_bytes!("../wasm/init.wasm");

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    kprintln!("Hello, {}!", "World");

    kernel::init();
    let allocator =
        unsafe { kernel::init_memory(boot_info).expect("Failed to initialize allocator") };

    let mut compiler = X86_64Compiler::new();
    compiler
        .parse(WASM_USERBOOT)
        .expect("Failed parsing userboot");
    let module = compiler.compile().expect("Failed compiling userboot");
    let instance = Instance::instantiate(&module, Vec::new(), &allocator)
        .expect("Failed to instantiate userboot");
    let user_init = instance
        .get_func_addr_from_name("init")
        .expect("Failed to retrieve 'init' from userboot instance");
    let vmctx = instance.get_vmctx_ptr();

    let result: u32;
    unsafe {
        asm!(
            "call {entry_point}",
            entry_point = in(reg) user_init,
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
