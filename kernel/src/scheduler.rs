//! Scheduler

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use wasm::FuncIndex;

use crate::wasm::Component;

/// A component scheduler.
///
/// Responsible for scheduling components onto the current CPU core.
pub struct Scheduler {
    queue: VecDeque<(Arc<Component>, FuncIndex)>,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            queue: VecDeque::new(),
        }
    }

    /// Schedules a component.
    pub fn enqueue(&mut self, component: Arc<Component>, func: FuncIndex) {
        self.queue.push_back((component, func));
    }

    /// Starts execution of component on this CPU core.
    pub fn run(&mut self) {
        if let Some((component, func)) = self.queue.pop_front() {
            match component.try_run(func) {
                crate::wasm::RunStatus::Ok => crate::kprintln!("Ran func {:?}", func),
                crate::wasm::RunStatus::Busy => crate::kprintln!("Failed to run {:?}", func),
            }
        }
        crate::kprintln!("Scheduler: done");
    }
}
