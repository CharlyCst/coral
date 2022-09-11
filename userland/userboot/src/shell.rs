//! Shell

use crate::vga;

pub struct Shell {
    #[allow(dead_code)]
    shell_start: usize,
    x: usize,
    y: usize,
    color: vga::ColorCode,
    prompt: vga::ColorCode,
}

impl Shell {
    pub fn new(shell_start: usize, color: vga::ColorCode) -> Self {
        Self {
            shell_start,
            x: 2,
            y: shell_start,
            color,
            prompt: color.with_foreground(vga::Color::Green),
        }
    }

    pub fn write(&mut self, string: &str) {
        for c in string.chars() {
            if c.is_ascii() {
                self.write_char(c);
            } else if c == '\n' {
                self.next_line();
            }
        }
    }

    pub fn writeln(&mut self, string: &str) {
        self.write(string);
        self.next_line();
    }

    #[allow(dead_code)]
    pub fn write_num(&mut self, num: u64) {
        const NB_DIGIT: u64 = 64 / 4;
        const MASK: u64 = 0xF;
        for i in (0..NB_DIGIT).rev() {
            let u4 = num >> (i * 4);
            let u4 = (u4 & MASK) as u8;
            let c = hex_u4(u4);
            self.write_char(c);
        }
    }

    pub fn input(&mut self, c: char) {
        if c == '\n' {
            self.prompt();
        } else if c.is_ascii() {
            self.write_char(c);
        }
    }

    pub fn prompt(&mut self) {
        self.next_line();
        vga::write_char(self.prompt.char(b'>'), 0, self.y);
    }

    pub fn flush(&self) {
        vga::flush();
    }

    fn write_char(&mut self, c: char) {
        vga::write_char(self.color.char(c as u8), self.x, self.y);
        self.next_char();
    }

    fn next_line(&mut self) {
        self.y += 1;
        self.x = 2;
    }

    fn next_char(&mut self) {
        self.x += 1;
        if self.x >= vga::BUFFER_WIDTH {
            self.next_line();
        }
    }
}

fn hex_u4(u4: u8) -> char {
    // Mask 4 most significant bit
    let u4 = u4 & 0x0F;

    // Hex encode
    match u4 {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'A',
        11 => 'B',
        12 => 'C',
        13 => 'D',
        14 => 'E',
        15 => 'F',
        _ => unreachable!(),
    }
}
