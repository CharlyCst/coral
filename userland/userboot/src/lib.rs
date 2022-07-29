const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;
const BUFFER_LEN: usize = BUFFER_HEIGHT * BUFFER_WIDTH;

static mut BUFFER: [ScreenChar; BUFFER_LEN] = [ScreenChar::black(); BUFFER_LEN];

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(foreground: Color, background: Color) -> Self {
        Self((background as u8) << 4 | (foreground as u8))
    }

    fn char(self, ascii_character: u8) -> ScreenChar {
        ScreenChar {
            ascii_character,
            color_code: self,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

impl ScreenChar {
    pub const fn black() -> Self {
        Self {
            ascii_character: 0,
            color_code: ColorCode::new(Color::Black, Color::Black),
        }
    }
}

#[no_mangle]
pub fn init() -> u32 {
    let color = ColorCode::new(Color::Pink, Color::Black);

    for (idx, c) in "Hello, World!".chars().enumerate() {
        if c.is_ascii() {
            let byte = c as u8;

            // SAFETY: we only have a single thread in webassembly.
            unsafe {
                BUFFER.get_mut(2 * BUFFER_WIDTH + idx + 1).and_then(|loc| {
                    *loc = color.char(byte);
                    Some(())
                });
            }
        }
    }

    unsafe {
        vma_write(0, 0, BUFFER.as_ptr() as u64, 0, BUFFER.len() as u64);
    }

    42
}

// ———————————————————————————————— Syscalls ———————————————————————————————— //

type ExternRef = u32;

#[link(wasm_import_module = "coral")]
extern "C" {
    fn vma_write(
        source: ExternRef,
        target: ExternRef,
        source_offset: u64,
        target_offset: u64,
        size: u64,
    );
}
