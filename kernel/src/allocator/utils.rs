//! Utilities for allocators

use spin::{Mutex, MutexGuard};

/// Align the given address `addr` upwards to alignment `align`.
///
/// Requires that `align` is a power of two.
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// A wrapper around Mutex to permit trait implementations.
pub struct Locked<A> {
    inner: Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> MutexGuard<A> {
        self.inner.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn align() {
        // Already aligned
        assert_eq!(align_up(0x1000, 1), 0x1000);
        assert_eq!(align_up(0x1000, 2), 0x1000);
        assert_eq!(align_up(0x1000, 4), 0x1000);
        assert_eq!(align_up(0x1000, 8), 0x1000);

        // Misaligned
        assert_eq!(align_up(0x1001, 2), 0x1002);
        assert_eq!(align_up(0x1001, 4), 0x1004);
        assert_eq!(align_up(0x1002, 4), 0x1004);
        assert_eq!(align_up(0x1003, 4), 0x1004);
        assert_eq!(align_up(0x1007, 8), 0x1008);
    }
}
