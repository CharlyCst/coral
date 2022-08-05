// ————————————————————————————————— Screen ————————————————————————————————— //

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

/// Write a character to the internal buffer.
fn write_char(c: ScreenChar, x: usize, y: usize) {
    // SAFETY: we only have a single thread in webassembly.
    unsafe {
        BUFFER.get_mut(BUFFER_WIDTH * y + x).and_then(|loc| {
            *loc = c;
            Some(())
        });
    }
}

/// Display the buffer to the screen.
fn flush() {
    unsafe {
        vma_write(0, 0, BUFFER.as_ptr() as u64, 0, BUFFER.len() as u64);
    }
}

// ——————————————————————————— Exported Functions ——————————————————————————— //

const COLOR: ColorCode = ColorCode::new(Color::Pink, Color::Black);

#[no_mangle]
pub fn init() -> u32 {
    for (idx, c) in "Coral - userboot".chars().enumerate() {
        if c.is_ascii() {
            write_char(COLOR.char(c as u8), idx + 1, 1);
        }
    }

    for x in 0..BUFFER_WIDTH {
        write_char(COLOR.char(b'_'), x, 2);
    }

    flush();

    42
}

static mut COUNTER: usize = 0;

#[no_mangle]
pub fn tick() {
    let counter = unsafe {
        COUNTER += 1;
        COUNTER
    };

    let char = match counter % 2 {
        0 => b'+',
        1 => b'-',
        _ => unreachable!(),
    };
    write_char(COLOR.char(char), BUFFER_WIDTH - 2, 1);
    flush();
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
