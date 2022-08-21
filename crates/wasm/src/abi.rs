//! WebAssembly ABI

use crate::types::ValueType;
use alloc::vec;
use alloc::vec::Vec;

/// A trait for base WebAssembly types.
///
/// SAFETY: This trait is already implemented for all basic types, custom WebAssembly types should
/// implement `WasmType` instead.
pub unsafe trait WasmBaseType: Send {
    type Abi: Copy;
    const VALUE_TYPE: ValueType;
}

macro_rules! impl_wasm_base_type {
    ($t:ty, $val:expr) => {
        unsafe impl WasmBaseType for $t {
            type Abi = $t;
            const VALUE_TYPE: ValueType = $val;
        }
    };
}

impl_wasm_base_type!(i32, ValueType::I32);
impl_wasm_base_type!(u32, ValueType::I32);
impl_wasm_base_type!(i64, ValueType::I64);
impl_wasm_base_type!(u64, ValueType::I64);

/// A WebAssembly externref type, ABI compatible with WebAssembly 64 bits references.
#[derive(Clone, Copy)]
pub enum ExternRef64 {}

unsafe impl WasmBaseType for ExternRef64 {
    type Abi = u64;
    const VALUE_TYPE: ValueType = ValueType::ExternRef;
}

/// A trait representing a value that is ABI compatible with a WebAssembly type and can be passed
/// across the WebAssembly/Native boundary.
///
/// SAFETY: this trait must only be implemented for types that are ABI compatible with one of the
/// WebAssembly types.
pub unsafe trait WasmType: Send + Copy {
    type Abi: WasmBaseType + Copy;

    fn into_abi(self) -> <Self::Abi as WasmBaseType>::Abi;
    fn from_abi(val: <Self::Abi as WasmBaseType>::Abi) -> Self;

    /// Returns the corresponding WebAssembly value type.
    fn ty() -> ValueType {
        Self::Abi::VALUE_TYPE
    }
}

/// WasmType implementation for types transparent to WebAssembly (e.g. u32, i64).
macro_rules! impl_wasm_type {
    ($t:ty) => {
        unsafe impl WasmType for $t {
            type Abi = $t;

            fn into_abi(self) -> Self::Abi {
                self
            }

            fn from_abi(val: Self::Abi) -> Self {
                val
            }
        }
    };
}

impl_wasm_type!(i32);
impl_wasm_type!(u32);
impl_wasm_type!(i64);
impl_wasm_type!(u64);

/// A trait representing parameters that can be passed to WebAssembly functions.
///
/// SAFETY: This trait must only be implemented for types that are ABI compatible with WebAssembly
/// built-in types.
pub unsafe trait WasmParams {
    type Abi: Copy;
    const NB_PARAMS: usize;

    fn ty() -> Vec<ValueType>;
}

// Special case: forward single value to generic tuple implementation.
unsafe impl<T> WasmParams for T
where
    T: WasmType,
{
    type Abi = <(T,) as WasmParams>::Abi;
    const NB_PARAMS: usize = <(T,) as WasmParams>::NB_PARAMS;

    fn ty() -> Vec<ValueType> {
        <(T,) as WasmParams>::ty()
    }
}

/// WasmParams implementation for tuples of WebAssembly compatible types.
macro_rules! impl_wasm_params {
    ($n:expr, $($t:ident)*) => {
        unsafe impl<$($t: WasmType,)*> WasmParams for ($($t,)*) {
            type Abi = ($($t::Abi,)*);
            const NB_PARAMS: usize = $n;

            fn ty() -> Vec<ValueType> {
                vec![$(<$t as WasmType>::ty(),)*]
            }
        }
    };
}

impl_wasm_params!(0,);
impl_wasm_params!(1, T1);
impl_wasm_params!(2, T1 T2);
impl_wasm_params!(3, T1 T2 T3);
impl_wasm_params!(4, T1 T2 T3 T4);
impl_wasm_params!(5, T1 T2 T3 T4 T5);
impl_wasm_params!(6, T1 T2 T3 T4 T5 T6);
impl_wasm_params!(7, T1 T2 T3 T4 T5 T6 T7);
impl_wasm_params!(8, T1 T2 T3 T4 T5 T6 T7 T8);

/// A trait representing results of a WebAssembly-compatible function.
///
/// SAFETY: this trait must only be implemented for types that are ABI compatible with WebAssembly
/// built-in types.
pub unsafe trait WasmResults: WasmParams + HostReturnAbi {}
unsafe impl<T> WasmResults for T where T: WasmParams + HostReturnAbi {}

/// A trait representing the Abi between host (native) and WebAssembly functions.
///
/// This corresponds to Cranelift's "Wasmtime" ABI, which follows the system ABI (so SystemV for
/// Coral) except that return values beyond the first are stored at a location passed as argument
/// (the return pointer). This is a workaround to compensate for the fact that SystemV ABI does not
/// allow multiple return values (and has complicated calling convention for returning structs).
pub unsafe trait HostReturnAbi {
    /// The actual value returned.
    type ReturnAbi: Copy;
    /// A pointer used to store the return values beyond the first, if any.
    type ReturnPtr: Copy;

    /// Format the return value according to the WebAssembly ABI used by the runtime.
    ///
    /// SAFETY: the pointer initially points to unitialized data, and **must** be initialized when
    /// the function returns if the return pointer is expected to points to valid (i.e. non-zero
    /// sized) data.
    unsafe fn into_abi(self, retptr: Self::ReturnPtr) -> Self::ReturnAbi;
}

// Forward implementation to the generic multi-value case.
unsafe impl<T> HostReturnAbi for T
where
    T: WasmType,
    T: Copy,
{
    type ReturnAbi = <(T,) as HostReturnAbi>::ReturnAbi;
    type ReturnPtr = <(T,) as HostReturnAbi>::ReturnPtr;

    unsafe fn into_abi(self, retptr: Self::ReturnPtr) -> Self::ReturnAbi {
        <(T,) as HostReturnAbi>::into_abi((self,), retptr)
    }
}

macro_rules! impl_host_return_abi {
    // Base case.
    ($ret:ident) => {
        unsafe impl HostReturnAbi for () {
            type ReturnAbi = ();
            type ReturnPtr = ();

            unsafe fn into_abi(self, _retptr: Self::ReturnPtr) -> Self::ReturnAbi {
                ()
            }
        }
    };

    // The first return value is passed directly, so no return pointers.
    ($ret:ident $t:ident) => {
        unsafe impl<$t: Copy> HostReturnAbi for ($t,) {
            type ReturnAbi = $t;
            type ReturnPtr = ();

            unsafe fn into_abi(self, _retptr: Self::ReturnPtr) -> Self::ReturnAbi {
                self.0
            }
        }
    };

    // Otherwise, the first return value is passed directly and the other through a pointer to a
    // reserved return area.
    ($ret:ident $t:ident $($u:ident)* ) => {
        /// A buffer for returned values.
        ///
        /// This is an implementation detail that may change at any time.
        #[allow(non_snake_case)]
        #[repr(C)]
        pub struct $ret<$($u,)*> {
            $($u: $u,)*
        }

        #[allow(non_snake_case)]
        unsafe impl <$t: Copy, $($u: Copy,)*> HostReturnAbi for ($t, $($u,)*) {
            type ReturnAbi = $t;
            type ReturnPtr = *mut  $ret<$($u,)*>;

            unsafe fn into_abi(self, retptr: Self::ReturnPtr) -> Self::ReturnAbi {
                let (val, $($u,)*) = self;
                *retptr = $ret {
                    $($u: $u,)*
                };
                val
            }
        }
    };
}

impl_host_return_abi!(Ret0);
impl_host_return_abi!(Ret1 T1);
impl_host_return_abi!(Ret2 T1 T2);
impl_host_return_abi!(Ret3 T1 T2 T3);
impl_host_return_abi!(Ret4 T1 T2 T3 T4);
impl_host_return_abi!(Ret5 T1 T2 T3 T4 T5);
impl_host_return_abi!(Ret6 T1 T2 T3 T4 T5 T6);
impl_host_return_abi!(Ret7 T1 T2 T3 T4 T5 T6 T7);
impl_host_return_abi!(Ret8 T1 T2 T3 T4 T5 T6 T7 T8);
impl_host_return_abi!(Ret9 T1 T2 T3 T4 T5 T6 T7 T8 T9);
