#![no_std]

mod keyboard;
mod shell;
mod syscalls;
mod vga;

const COLOR: vga::ColorCode = vga::ColorCode::new(vga::Color::Pink, vga::Color::Black);
static mut SHELL: Option<shell::Shell> = None;

#[no_mangle]
pub fn init() -> u32 {
    vga::write_str("Coral - userboot", COLOR, 1, 1);
    for x in 0..vga::BUFFER_WIDTH {
        vga::write_char(COLOR.char(b'_'), x, 2);
    }
    vga::flush();

    let mut console = shell::Shell::new(4, COLOR);

    // Test syscall:
    let wasm: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,
        0x03, 0x02, 0x01, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b,
    ];
    unsafe {
        console.write("Create module:      ");
        let (module, result) = syscalls::module_create(0, wasm.as_ptr() as u64, wasm.len() as u64);
        console.writeln(result.str());
        console.write("Create component:   ");
        let (component, result) = syscalls::component_create();
        console.writeln(result.str());
        console.write("Instantiate module: ");
        let (result, _) = syscalls::component_add_instance(component, module);
        console.writeln(result.str());
        console.prompt();
        console.flush();
    }

    unsafe {
        SHELL = Some(console);
    }

    42
}

static mut COUNTER: usize = 0;

#[no_mangle]
pub fn tick() {
    let counter = unsafe {
        COUNTER += 1;
        COUNTER
    };

    let char = match (counter / 2) % 5 {
        0 => b'_',
        1 => b'-',
        2 => b'+',
        3 => b'*',
        4 => b'!',
        _ => unreachable!(),
    };
    vga::write_char(COLOR.char(char), vga::BUFFER_WIDTH - 2, 1);
    vga::flush();
}

#[no_mangle]
pub fn press_key(scancode: u8) {
    let key = match keyboard::process_event(scancode) {
        Some(key) => key,
        None => return,
    };

    if let keyboard::DecodedKey::Unicode(char) = key {
        if char.is_ascii() {
            let console = unsafe {
                if let Some(ref mut console) = SHELL {
                    console
                } else {
                    return;
                }
            };
            console.input(char);
            console.flush();
        }
    }
}

// ————————————————————————————— Panic Handler —————————————————————————————— //

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
