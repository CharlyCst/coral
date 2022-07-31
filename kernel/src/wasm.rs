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

        self.call(func, &[]);

        // Release component
        self.busy.store(false, Ordering::SeqCst);
        RunStatus::Ok
    }

    /// Call an instance function using the SytemV ABI.
    ///
    /// See [OsDev wiki](https://wiki.osdev.org/System_V_ABI), [(old but rendered)
    /// spec](https://www.uclibc.org/docs/psABI-x86_64.pdf), and [newer
    /// spec](https://gitlab.com/x86-psABIs).
    fn call(&self, func: FuncIndex, args: &[u64]) {
        // Instance pointers
        let func_ptr = self.instance.get_func_addr_by_index(func);
        let func_ty = self.instance.get_func_type_by_index(func);
        let vmctx = self.instance.get_vmctx_ptr() as u64;

        assert_eq!(
            func_ty.args().len(),
            args.len(),
            "Mismatching types, should have been typechecked earlier!"
        );
        assert!(
            func_ty.ret().len() <= 2,
            "Returning more than 2 values from instances is not yet supported"
        );

        // Registers used to pass arguments
        let rdi;
        let mut rsi = 0;
        let mut rdx = 0;
        let mut rcx = 0;
        let mut r8 = 0;
        let mut r9 = 0;

        match args.len() {
            0 => rdi = vmctx,
            1 => {
                rdi = vmctx;
                rsi = args[0];
            }
            2 => {
                rdi = vmctx;
                rsi = args[0];
                rdx = args[1];
            }
            3 => {
                rdi = vmctx;
                rsi = args[0];
                rdx = args[1];
                rcx = args[2];
            }
            4 => {
                rdi = vmctx;
                rsi = args[0];
                rdx = args[1];
                rcx = args[2];
                r8 = args[3];
            }
            5 => {
                rdi = vmctx;
                rsi = args[0];
                rdx = args[1];
                rcx = args[2];
                r8 = args[3];
                r9 = args[4];
            }
            // Other registers must be passed on the stack.
            // We will implement that once each component/instance has its own stack.
            _ => todo!("At most 5 arguments can be passed for now"),
        }

        unsafe {
            asm!(
                "call {func_ptr}",
                func_ptr = in(reg) func_ptr,
                // Function arguments
                in("rdi") rdi,
                in("rsi") rsi,
                in("rdx") rdx,
                in("rcx") rcx,
                in("r8")  r8,
                in("r9")  r9,
                // Clobbered registers
                out("rax") _,
                out("r10") _,
                out("r11") _,
            );
        }
    }
}
