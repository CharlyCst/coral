//! Native Functions

use crate::abi::{WasmParams, WasmResults};
use crate::types::FuncType;
use core::marker::PhantomData;

/// A native function that can seamelessly be called from WebAssembly.
///
/// Contratry to other native functions, this function is guaranteed to implement the WebAssembly
/// ABI used by the Coral runtime, and can therefore be called directly, without the need for any
/// trampoline.
/// The primary usage of native functions is to implement system calls and runtime functions.
pub struct NativeFunc<Params, Results> {
    _signature: PhantomData<fn(Params) -> Results>,
    ptr: *const u8,
}

impl<Params, Results> NativeFunc<Params, Results> {
    pub const unsafe fn new(ptr: *const u8) -> Self {
        Self {
            _signature: PhantomData,
            ptr,
        }
    }

    pub fn ptr(&self) -> *const u8 {
        self.ptr
    }
}

impl<Params, Results> NativeFunc<Params, Results>
where
    Params: WasmParams,
    Results: WasmResults,
{
    pub fn ty(&self) -> FuncType {
        FuncType::new(Params::ty(), Results::ty())
    }
}

unsafe impl<P, R> Sync for NativeFunc<P, R> {}

/// Converts a native Rust function into a function that can be seamelessly called from
/// WebAssembly.
///
/// The only cost is the ABI change, which might be in part optimized-out by the compiler, making
/// those calls pretty fast.
///
/// NOTE: I'm not very satisfied with this macro, it's not ergonomic at all and the implementation
/// is not pretty (see all the matching rules...). Maybe this would be more suited for a proc
/// macro, that can parses the params itself and directly anotate a function.
#[macro_export(local_inner_macro)]
macro_rules! as_native_func {
    // Match for different number of arguments
    ($func:ident; $static:ident) => {
        as_native_func!(inner $func; $static; args_names: ; args_types: ; ret: ());
    };
    ($func:ident; $static:ident; ret: $ret:tt) => {
        as_native_func!(inner $func; $static; args_names: ; args_types: ; ret: $ret);
    };
    ($func:ident; $static:ident; args: $arg1:ident; ret: $ret:tt) => {
        as_native_func!(inner $func; $static; args_names: a1; args_types: $arg1; ret: $ret);
    };
    ($func:ident; $static:ident; args: $arg1:ident $arg2:ident; ret: $ret:tt) => {
        as_native_func!(inner $func; $static; args_names: a1 a2; args_types: $arg1 $arg2; ret: $ret);
    };
    ($func:ident; $static:ident; args: $arg1:ident $arg2:ident $arg3:ident; ret: $ret:tt) => {
        as_native_func!(inner $func; $static; args_names: a1 a2 a3; args_types: $arg1 $arg2 $arg3; ret: $ret);
    };
    ($func:ident; $static:ident; args: $arg1:ident $arg2:ident $arg3:ident $arg4:ident; ret: $ret:tt) => {
        as_native_func!(inner $func; $static; args_names: a1 a2 a3 a4; args_types: $arg1 $arg2 $arg3 $arg4; ret: $ret);
    };
    ($func:ident; $static:ident; args: $arg1:ident $arg2:ident $arg3:ident $arg4:ident $arg5:ident; ret: $ret:tt) => {
        as_native_func!(inner $func; $static; args_names: a1 a2 a3 a4 a5; args_types: $arg1 $arg2 $arg3 $arg4 $arg5; ret: $ret);
    };

    // Main body, where we have both arguments types and names
    (inner $func:ident; $static:ident; args_names: $($args_n:ident)*; args_types: $($args_t:ident)*; ret: $ret:tt) => {
        static $static: $crate::NativeFunc<($($args_t,)*), $ret> = {
            // NOTE: taking `()` as argument is not FFI-safe, hence the `allow` clause.
            // Here se rely on the fact that `()` arguments are optimized out so that the function
            // matches the Cranlift WasmtimeSysV ABI.
            #[allow(improper_ctypes_definitions)]
            unsafe extern "sysv64" fn wasm_to_host(
                $($args_n: <<$args_t as $crate::WasmType>::Abi as $crate::WasmBaseType>::Abi,)*
                retptr: <$ret as $crate::HostReturnAbi>::ReturnPtr,
                _vmctx: *mut u8,
            ) -> <$ret as $crate::HostReturnAbi>::ReturnAbi
            {
                let ret = $func($(<$args_t as $crate::WasmType>::from_abi($args_n),)*);
                <$ret as $crate::HostReturnAbi>::into_abi(ret, retptr)
            }

            unsafe { $crate::NativeFunc::new(wasm_to_host as *const u8) }
        };
    };
}

#[cfg(test)]
mod tests {
    use crate::{as_native_func, FuncType, ValueType};
    use alloc::vec;

    #[test]
    fn native_func() {
        fn func_1() {}
        fn func_2(_a: u32) {}
        fn func_3(_a: i32, _b: u64) {}
        fn func_4() -> u64 {
            0
        }
        fn func_5() -> (i32, u64) {
            (0, 0)
        }
        fn func_6(_a: i32, _b: u32) -> (i32, i32, i32) {
            (0, 0, 0)
        }

        as_native_func!(func_1; F1; ret: ());
        as_native_func!(func_2; F2; args: u32; ret: ());
        as_native_func!(func_3; F3; args: i32 u64; ret: ());
        as_native_func!(func_4; F4; ret: u64);
        as_native_func!(func_5; F5; ret: (i32, u64));
        as_native_func!(func_6; F6; args: i32 u32; ret: (i32, i32, i32));

        assert!(F1.ty().eq(&FuncType::new(vec![], vec![])));
        assert!(F2.ty().eq(&FuncType::new(vec![ValueType::I32], vec![])));
        assert!(F3
            .ty()
            .eq(&FuncType::new(vec![ValueType::I32, ValueType::I64], vec![])));
        assert!(F4.ty().eq(&FuncType::new(vec![], vec![ValueType::I64])));
        assert!(F5
            .ty()
            .eq(&FuncType::new(vec![], vec![ValueType::I32, ValueType::I64])));
        assert!(F6.ty().eq(&FuncType::new(
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32, ValueType::I32, ValueType::I32]
        )));
    }
}
