#![no_std]

mod keyboard;
mod syscalls;
mod vga;

// ——————————————————————————— Exported Functions ——————————————————————————— //

const COLOR: vga::ColorCode = vga::ColorCode::new(vga::Color::Pink, vga::Color::Black);

#[no_mangle]
pub fn init() -> u32 {
    for (idx, c) in "Coral - userboot".chars().enumerate() {
        if c.is_ascii() {
            vga::write_char(COLOR.char(c as u8), idx + 1, 1);
        }
    }

    for x in 0..vga::BUFFER_WIDTH {
        vga::write_char(COLOR.char(b'_'), x, 2);
    }

    vga::flush();

    // Test syscall:
    let wasm: &[u8] = &[
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,
        0x03, 0x02, 0x01, 0x00, 0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b,
    ];
    unsafe { syscalls::module_create(0, wasm.as_ptr() as u64, wasm.len() as u64) };

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

static mut CURSOR_X: usize = 0;
static mut CURSOR_Y: usize = 4;

#[no_mangle]
pub fn press_key(scancode: u8) {
    let key = match keyboard::process_event(scancode) {
        Some(key) => key,
        None => return,
    };

    if let keyboard::DecodedKey::Unicode(char) = key {
        if char.is_ascii() {
            let (x, y) = get_cursor();
            vga::write_char(COLOR.char(char as u8), x, y);
            vga::flush();
            move_cursor();
        }
    }
}

fn get_cursor() -> (usize, usize) {
    // SAFETY: There is no concurrency in this program, hence not race conditions
    unsafe { (CURSOR_X, CURSOR_Y) }
}

fn move_cursor() {
    // SAFETY: There is no concurrency in this program, hence not race conditions
    unsafe {
        CURSOR_X += 1;

        if CURSOR_X >= vga::BUFFER_WIDTH {
            if CURSOR_Y + 1 < vga::BUFFER_HEIGHT {
                CURSOR_X = 0;
                CURSOR_Y += 1;
            }
        }
    }
}

// ————————————————————————————— Panic Handler —————————————————————————————— //

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
