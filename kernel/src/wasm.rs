//! WebAssembly Abstractions

use alloc::sync::Arc;
use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::memory::Vma;
use wasm::{FuncIndex, Instance};

pub struct Component {
    instance: Instance<Arc<Vma>>,
    busy: AtomicBool,
}

#[must_use]
pub enum RunStatus {
    Ok,
    Busy,
}

impl RunStatus {
    /// Acknowledge run status.
    pub fn ok(self) {}
}

impl Component {
    pub fn new(instance: Instance<Arc<Vma>>) -> Self {
        let component = Self {
            instance,
            busy: AtomicBool::new(false),
        };
        if let Some(func) = component.instance.get_start() {
            // We know the component is not busy yet
            component.try_run(func).ok();
        }

        component
    }

    pub fn try_run(&self, func: FuncIndex) -> RunStatus {
        // Try to acquire component
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            == Err(false)
        {
            return RunStatus::Busy;
        }

        let ptr = self.instance.get_func_addr_by_index(func);
        let vmctx = self.instance.get_vmctx_ptr();
        unsafe {
            asm!(
                "call {ptr}",
                ptr = in(reg) ptr,
                in("rdi") vmctx,
                out("rax") _, // (first) return value gues here
            );
        }

        // Release component
        self.busy.store(false, Ordering::SeqCst);
        RunStatus::Ok
    }
}
