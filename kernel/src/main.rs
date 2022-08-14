#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(kernel::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::sync::Arc;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use core::ptr::NonNull;

use compiler::{Compiler, X86_64Compiler};
use kernel::kprintln;
use kernel::memory::Vma;
use kernel::runtime::{KoIndex, Runtime, ACTIVE_VMA};
use kernel::wasm::Args;

/// The first user program to run, expected to boostrap userspace.
const WASM_USERBOOT: &'static [u8] = std::include_bytes!("../wasm/userboot.wasm");

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    kprintln!("Hello, {}!", "World");

    kernel::init();
    let allocator =
        unsafe { kernel::init_memory(boot_info).expect("Failed to initialize allocator") };

    // Run tests and exit when called with `cargo test`
    #[cfg(test)]
    test_main();

    // Register runtime compiler
    let compiler = Box::new(|wasm: &[u8]| {
        let mut compiler = X86_64Compiler::new();
        compiler
            .parse(wasm)
            .map_err(|err| kprintln!("Failed to parse: {:?}", err))?;
        compiler
            .compile()
            .map_err(|err| kprintln!("Failed to compule: {:?}", err))
    });
    kernel::runtime::register_compiler(compiler);

    // Initialize the Coral native module
    let runtime = Runtime::new(allocator);
    let vga_buffer =
        unsafe { Vma::from_raw(NonNull::new(0xb8000 as *mut u8).unwrap(), 80 * 25 * 2) };
    let vga_idx = ACTIVE_VMA.insert(Arc::new(vga_buffer)).into_externref();
    let coral_handles_table = vec![vga_idx];
    let coral_module = kernel::syscalls::build_syscall_module(coral_handles_table);
    let coral_instance = runtime
        .instantiate(&coral_module, Vec::new())
        .expect("Failed to instantiate coral syscalls module");

    // Compile & initialize userboot
    let mut compiler = X86_64Compiler::new();
    compiler
        .parse(WASM_USERBOOT)
        .expect("Failed parsing userboot");
    let user_module = compiler.compile().expect("Failed compiling userboot");
    let userboot = runtime
        .instantiate(&user_module, vec![("coral", coral_instance)])
        .expect("Failed to instantiate userboot");
    let userboot_init = userboot
        .get_func_index_by_name("init")
        .expect("Failed to retrieve 'init' from userboot instance");
    let userboot_tick = userboot
        .get_func_index_by_name("tick")
        .expect("Failed to retrieve 'tick' from userboot instance");
    let userboot_key = userboot
        .get_func_index_by_name("press_key")
        .expect("Failes to retrieve 'press_key' from userboot instance");
    let component = Arc::new(kernel::wasm::Component::new(userboot));

    let scheduler = Arc::new(kernel::scheduler::Scheduler::new());

    // Keyboard events
    let keyboard_dispatcher = Arc::new(kernel::events::EventDispatcher::new(128));
    let keyboard_source = keyboard_dispatcher.source().clone();
    kernel::events::KEYBOARD_EVENTS.initialize(keyboard_source);
    keyboard_dispatcher.add_listener(component.clone(), userboot_key);
    scheduler.schedule(keyboard_dispatcher.dispatch(scheduler.clone()));

    // Timer events
    let timer_dispatcher = Arc::new(kernel::events::EventDispatcher::new(128));
    let timer_source = timer_dispatcher.source().clone();
    kernel::events::TIMER_EVENTS.initialize(timer_source);
    timer_dispatcher.add_listener(component.clone(), userboot_tick);
    scheduler.schedule(timer_dispatcher.dispatch(scheduler.clone()));

    // Schedule userboot
    scheduler.schedule(component.run(userboot_init, Args::new()));
    scheduler.run();
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
