//! Keyboard Handler

use pc_keyboard::layouts::Azerty;
use pc_keyboard::{HandleControl, Keyboard, ScancodeSet1};

pub use pc_keyboard::DecodedKey;

// NOTE: We require an option here as Keyboard::new is not yet const fn (I filled a PR for that).
static mut KEYBOARD: Option<Keyboard<Azerty, ScancodeSet1>> = None;

pub fn process_event(scancode: u8) -> Option<DecodedKey> {
    // SAFETY: This program is single threaded with no concurrency, so there can't be race
    // conditions
    let keyboard = unsafe { &mut KEYBOARD };

    // Initialize keyboard if not already done
    if keyboard.is_none() {
        *keyboard = Some(Keyboard::new(Azerty, ScancodeSet1, HandleControl::Ignore));
    }

    // Get the keyboard
    let keyboard = match keyboard {
        Some(keyboard) => keyboard,
        None => return None,
    };

    match keyboard.add_byte(scancode) {
        Ok(Some(key_event)) => keyboard.process_keyevent(key_event),
        _ => None,
    }
}
