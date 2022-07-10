//! WebAssembly Runtime
//!
//! This module provides the necessary runtime support for proper instantiation and execution of
//! userspace modules, as well as support for managing kernel objects.

mod kernel_objects;
mod runtime;

pub use kernel_objects::{VmaIndex, ACTIVE_VMA, KoIndex};
pub use runtime::Runtime;

