#![no_std]
#![feature(allocator_api)]

extern crate alloc;

mod compiler;
mod env;

pub use compiler::X86_64Compiler;

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "oceanc"))]
pub mod userspace_alloc;
