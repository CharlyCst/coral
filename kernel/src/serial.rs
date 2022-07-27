use core::fmt;
use core::fmt::Write;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;
use x86_64::instructions::interrupts::without_interrupts;

lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[macro_export]
macro_rules! debug_print {
    ($($args:tt)*) => {
        $crate::serial::_print(core::format_args!($($args)*));
    };
}

#[macro_export]
macro_rules! debug_println {
    () => ($crate::debug_print!("\n"));
    ($fmt:expr) => ($crate::debug_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::debug_print!(concat!($fmt, "\n"), $($arg)*));
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        if cfg! (not(test)) {
            $crate::serial::_print(core::format_args!($($arg)*))
        }
    };
}

#[macro_export]
macro_rules! kprintln {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", core::format_args!($($arg)*)))
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    without_interrupts(|| {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("Printing to serial failed");
    });
}
