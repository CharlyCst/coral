//! VGA driver

use crate::syscalls;

pub const BUFFER_HEIGHT: usize = 25;
pub const BUFFER_WIDTH: usize = 80;
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
pub struct ColorCode(u8);

impl ColorCode {
    pub const fn new(foreground: Color, background: Color) -> Self {
        Self((background as u8) << 4 | (foreground as u8))
    }

    pub fn char(self, ascii_character: u8) -> ScreenChar {
        ScreenChar {
            ascii_character,
            color_code: self,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct ScreenChar {
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
pub fn write_char(c: ScreenChar, x: usize, y: usize) {
    // SAFETY: we only have a single thread in webassembly.
    unsafe {
        BUFFER.get_mut(BUFFER_WIDTH * y + x).and_then(|loc| {
            *loc = c;
            Some(())
        });
    }
}

/// Display the buffer to the screen.
pub fn flush() {
    unsafe {
        syscalls::vma_write(0, 0, BUFFER.as_ptr() as u64, 0, BUFFER.len() as u64);
    }
}


