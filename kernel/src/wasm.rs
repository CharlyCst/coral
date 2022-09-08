//! WebAssembly Abstractions

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;

use crate::kprintln;
use crate::memory::Vma;
use crate::runtime::get_runtime;
use crate::scheduler::Task;
use collections::{entity_impl, PrimaryMap};
use wasm::{FuncIndex, Instance, Module, ModuleResult};

use spin::{Mutex, MutexGuard};

pub struct Component {
    inner: Mutex<InnerComponent>,
}

struct InnerComponent {
    /// The instancees within this component.
    instances: PrimaryMap<InstanceIndex, Arc<Instance<Arc<Vma>>>>,
    /// The available imports for the next module instantiation.
    next_imports: Vec<(String, Arc<Instance<Arc<Vma>>>)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(transparent)]
pub struct InstanceIndex(u32);
entity_impl!(InstanceIndex);

/// The ID of a function withing a component.
#[derive(Clone, Copy)]
pub struct ComponentFunc {
    instance: InstanceIndex,
    func: FuncIndex,
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
    pub fn new() -> Self {
        let component = Self {
            inner: Mutex::new(InnerComponent {
                instances: PrimaryMap::new(),
                next_imports: Vec::new(),
            }),
        };

        component
    }

    /// Add an import, which can be used by instances during future instantiations.
    pub fn push_import(&self, name: String, idx: InstanceIndex) {
        let mut component = self.lock();
        let instance = Arc::clone(&component.instances[idx]);
        component.next_imports.push((name, instance));
    }

    /// Add an instance to this component.
    pub fn add_instance(&self, module: &impl Module) -> ModuleResult<InstanceIndex> {
        let runtime = get_runtime();
        let mut component = self.lock();
        // TODO: find a more elegant way of resolving imports
        let imports: Vec<(&str, Arc<Instance<Arc<Vma>>>)> = component
            .next_imports
            .iter()
            .map(|(name, instance)| (name.as_str(), instance.clone()))
            .collect();
        let instance = Arc::new(Instance::instantiate(module, &imports, runtime)?);
        let idx = component.instances.push(instance);
        if let Some(func) = component.instances[idx].get_start() {
            let func = ComponentFunc {
                instance: idx,
                func,
            };
            match self.try_run(func, &Args::new()) {
                RunStatus::Ok => {} // Fine
                RunStatus::Busy => {
                    // TODO: How can we run init while the component is already executing?
                    kprintln!("WARNING: component is buzy, instance can't be initialized");
                    todo!("Handle buzy component initialization");
                }
            }
        }
        Ok(idx)
    }

    /// Get a function handle.
    pub fn get_func(&self, func: &str, instance: InstanceIndex) -> Option<ComponentFunc> {
        let component = self.lock();
        match component.instances[instance].get_func_index_by_name(func) {
            Some(func) => Some(ComponentFunc { instance, func }),
            None => None,
        }
    }

    pub fn try_run(&self, func: ComponentFunc, args: &Args) -> RunStatus {
        let mut component = match self.inner.try_lock() {
            Some(inner) => inner,
            None => {
                return RunStatus::Busy;
            }
        };

        component.call(func, args);

        RunStatus::Ok
    }

    pub fn run(self: Arc<Self>, func: ComponentFunc, args: Args) -> Task {
        Task::new(self.run_promise(func, args))
    }

    /// Run the given function from a component.
    async fn run_promise(self: Arc<Self>, func: ComponentFunc, args: Args) {
        match self.try_run(func, &args) {
            RunStatus::Ok => {}
            RunStatus::Busy => todo!("Handle busy components"),
        }
    }

    fn lock(&self) -> MutexGuard<InnerComponent> {
        self.inner.lock()
    }
}

impl InnerComponent {
    /// Call an instance function using the SytemV ABI.
    ///
    /// See [OsDev wiki](https://wiki.osdev.org/System_V_ABI), [(old but rendered)
    /// spec](https://www.uclibc.org/docs/psABI-x86_64.pdf), and [newer
    /// spec](https://gitlab.com/x86-psABIs).
    fn call(&mut self, func: ComponentFunc, args: &Args) {
        let args = args.as_slice();

        // Instance pointers
        let instance = &self.instances[func.instance];
        let func_ptr = instance.get_func_addr_by_index(func.func);
        let func_ty = instance.get_func_type_by_index(func.func);
        let vmctx = instance.get_vmctx_ptr() as u64;

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
