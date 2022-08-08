//! WebAssembly Abstractions

use alloc::sync::Arc;
use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::memory::Vma;
use crate::scheduler::Task;
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
            component.try_run(func, &Args::new()).ok();
        }

        component
    }

    pub fn try_run(&self, func: FuncIndex, args: &Args) -> RunStatus {
        // Try to acquire component
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            == Err(false)
        {
            return RunStatus::Busy;
        }

        self.call(func, args);

        // Release component
        self.busy.store(false, Ordering::SeqCst);
        RunStatus::Ok
    }

    /// Call an instance function using the SytemV ABI.
    ///
    /// See [OsDev wiki](https://wiki.osdev.org/System_V_ABI), [(old but rendered)
    /// spec](https://www.uclibc.org/docs/psABI-x86_64.pdf), and [newer
    /// spec](https://gitlab.com/x86-psABIs).
    fn call(&self, func: FuncIndex, args: &Args) {
        let args = args.as_slice();

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
                rdi = args[0];
                rsi = vmctx;
            }
            2 => {
                rdi = args[0];
                rsi = args[1];
                rdx = vmctx;
            }
            3 => {
                rdi = args[0];
                rsi = args[1];
                rdx = args[2];
                rcx = vmctx;
            }
            4 => {
                rdi = args[0];
                rsi = args[1];
                rdx = args[2];
                rcx = args[3];
                r8 = vmctx;
            }
            5 => {
                rdi = args[0];
                rsi = args[1];
                rdx = args[2];
                rcx = args[3];
                r8 = args[4];
                r9 = vmctx;
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

    pub fn run(self: Arc<Self>, func: FuncIndex, args: Args) -> Task {
        Task::new(self.run_promise(func, args))
    }

    /// Run the given function from a component.
    async fn run_promise(self: Arc<Self>, func: FuncIndex, args: Args) {
        match self.try_run(func, &args) {
            RunStatus::Ok => {}
            RunStatus::Busy => todo!("Handle busy components"),
        }
    }
}

// ——————————————————————————————— Arguments ———————————————————————————————— //

/// Wasm function call arguments.
#[derive(Debug, Clone)]
pub struct Args {
    // We support at most 5 arguments for now
    args: [u64; 5],
    len: usize,
}

/// A trait for values that can be converted to a sequence of Wasm arguments.
pub trait AsArgs {
    fn as_args(&self) -> Args;
}

/// A trait for values that can be converted to a single Wasm argument.
pub trait AsArg {
    fn as_arg(&self) -> u64;
}

impl Args {
    /// Creates an empty argument object.
    pub fn new() -> Self {
        Self {
            args: [0x41; 5],
            len: 0,
        }
    }

    /// Adds an argument.
    pub fn push<T>(mut self, arg: T) -> Self
    where
        T: AsArg,
    {
        if self.len >= self.args.len() {
            panic!("Too many arguments, we support at most 5 for now");
        }
        self.args[self.len] = arg.as_arg();
        self.len += 1;
        self
    }

    fn as_slice(&self) -> &[u64] {
        &self.args[0..self.len]
    }
}

impl<T> AsArgs for T
where
    T: AsArg,
{
    fn as_args(&self) -> Args {
        Args::new().push(self.as_arg())
    }
}

impl AsArgs for () {
    fn as_args(&self) -> Args {
        Args::new()
    }
}

impl AsArg for u64 {
    fn as_arg(&self) -> u64 {
        *self
    }
}

impl AsArg for u32 {
    fn as_arg(&self) -> u64 {
        *self as u64
    }
}

impl AsArg for u16 {
    fn as_arg(&self) -> u64 {
        *self as u64
    }
}

impl AsArg for u8 {
    fn as_arg(&self) -> u64 {
        *self as u64
    }
}
