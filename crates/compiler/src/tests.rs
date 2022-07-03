use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::arch::asm;

use wat;

use crate::alloc;
use crate::alloc::string::String;
use crate::compiler;
use crate::compiler::Compiler;
use crate::userspace_alloc::{LibcAllocator, MMapArea};
use wasm::{
    ExternRef64, Instance, MemoryArea, Module, NativeModuleBuilder, RawFuncPtr, WasmModule,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
struct ExternRef(*const u8);

impl ExternRef64 for ExternRef {
    fn to_u64(self) -> u64 {
        self.0 as u64
    }
}

#[test]
fn the_answer() {
    let module = compile(
        r#"
        (module
            (func $the_answer (result i32)
                i32.const 42
            )
            (export "main" (func $the_answer))
        )
    "#,
    );
    assert_eq!(execute_0(module), 42);
}

#[test]
fn zeroed_memory() {
    let module = compile(
        r#"
        (module
            (func $zero (result i32)
                i32.const 0
                i32.load
            )
            (memory $mem 1 1)
            (export "main" (func $zero))
        )
    "#,
    );
    assert_eq!(execute_0(module), 0);
}

#[test]
fn store_and_load() {
    let module = compile(
        r#"
        (module
            (func $store_and_load (result i32)
                i32.const 0 ;; Memory address for the store
                i32.const 42
                i32.store

                i32.const 0 ;; Memory address for the load
                i32.load
            )
            (memory $mem 1 1) ;; Fixed size heap
            (export "main" (func $store_and_load))
        )
    "#,
    );
    assert_eq!(execute_0(module), 42);
}

#[test]
fn import_memory() {
    let module = compile(
        r#"
        (module
            (type $t (func))
            (import "answer" "set_answer"
                (func $set_answer (type $t))
            )
            (import "answer" "memory"
                (memory $mem 1)
            )
            (func $main (result i32)
                call $set_answer
                i32.const 0
                i32.load
            )
            (export "main" (func $main))
        )
        "#,
    );
    let imported_module = compile(
        r#"
        (module
            (func $set_answer
                i32.const 0
                i32.const 42
                i32.store
            )
            (memory $mem 1 1)
            (export "memory" (memory $mem))
            (export "set_answer" (func $set_answer))
        )
    "#,
    );
    let answer = execute_0_deps(module, vec![("answer", imported_module)]);
    assert_eq!(answer.return_value, 42);
}

// // The Wasm proposal for multi memory is not yet standardized (phase 3 out of 5 at the time of
// // writing).
//
// #[test]
// fn multi_memory() {
//     let module = compile(
//         r#"
//         (module
//             (func $zero (result i32)
//                 i32.const 0
//                 i32.load
//             )
//             (memory $mem_a 1 1)
//             (memory $mem_b 1 1)
//             (export "main" (func $zero))
//         )
//     "#,
//     );
//     assert_eq!(execute_0(module), 0);
// }

#[test]
fn call() {
    let module = compile(
        r#"
        (module
            (func $add_and_square (param $arg1 i32) (param $arg2 i32) (result i32)
                local.get $arg1
                local.get $arg2
                i32.add

                call $square
            )
            (func $square (param $arg i32) (result i32)
                local.get $arg
                local.get $arg
                i32.mul
            )
            (export "main" (func $add_and_square))
        )
    "#,
    );
    assert_eq!(execute_2(module, 2, 3), 25);
}

#[test]
fn import() {
    let module = compile(
        r#"
        (module
            (import "answer" "the_answer"
                (func $the_answer (type $t))
            )
            (type $t (func (result i32)))
            (func $call_imported (result i32)
                call $the_answer
            )
            (export "main" (func $call_imported))
        )
        "#,
    );
    let imported_module = compile(
        r#"
        (module
            (func $the_answer (result i32)
                i32.const 42
            )
            (export "the_answer" (func $the_answer))
        )
    "#,
    );
    let answer = execute_0_deps(module, vec![("answer", imported_module)]);
    assert_eq!(answer.return_value, 42);
}

#[test]
fn import_native_func() {
    let module = compile(
        r#"
        (module
            (import "answer" "the_answer"
                (func $the_answer (type $t))
            )
            (type $t (func (result i32)))
            (func $call_imported (result i32)
                call $the_answer
            )
            (export "main" (func $call_imported))
        )
        "#,
    );

    extern "sysv64" fn foreign_func(_vmctx: u64) -> i32 {
        42
    }
    let imported_module = unsafe {
        let the_answer = RawFuncPtr::new(foreign_func as *mut u8);
        NativeModuleBuilder::new()
            .add_func(String::from("the_answer"), the_answer)
            .build()
    };
    let answer = execute_0_deps(module, vec![("answer", imported_module)]);
    assert_eq!(answer.return_value, 42);
}

#[test]
fn import_native_table() {
    let module = compile(
        r#"
        (module
            (import "native_mod" "table"
                (table $table 2 2 externref)
            )
            (func $main (result i32)
                i32.const 42
            )
            (export "main" (func $main))
        )
        "#,
    );

    let ref1 = ExternRef(0x42 as *const u8);
    let ref2 = ExternRef(0x54 as *const u8);
    let table = vec![ref1, ref2];
    let imported_module = NativeModuleBuilder::new()
        .add_table(String::from("table"), table)
        .build();
    let answer = execute_0_deps(module, vec![("native_mod", imported_module)]);
    assert_eq!(answer.return_value, 42);
}

#[test]
fn table_get_set() {
    // Swith the position of two table items
    let module = compile(
        r#"
        (module
            (import "native_mod" "table"
                (table $table 2 2 externref)
            )
            (func $main (result i32)
                i32.const 1
                i32.const 0
                table.get $table
                i32.const 0
                i32.const 1
                table.get $table
                table.set $table
                table.set $table

                i32.const 42
            )
            (export "main" (func $main))
            (export "table" (table $table))
        )
        "#,
    );

    let ref1 = ExternRef(0x42 as *const u8);
    let ref2 = ExternRef(0x54 as *const u8);
    let table = vec![ref1, ref2];
    let imported_module = NativeModuleBuilder::new()
        .add_table(String::from("table"), table)
        .build();
    let answer = execute_0_deps(module, vec![("native_mod", imported_module)]);
    assert_eq!(answer.return_value, 42);
    let table = answer.instance.get_table_by_name("table");
    assert_eq!(table, Some(&vec![0x54, 0x42].into_boxed_slice()));
}

#[test]
fn context_switch() {
    // The memory must not be shared between instances!
    let module = compile(
        r#"
        (module
            (import "mod" "zero"
                (func $zero (type $t))
            )
            (type $t (func (result i32)))
            (func $call_imported (result i32)
                i32.const 0  ;; Memory addres
                i32.const 14 ;; Random value
                i32.store

                call $zero   ;; Should return 0 if memory are not shared
                i32.const 42
                i32.add      ;; Should return 42 if memory are not shared
            )
            (memory $mem 1 1)
            (export "main" (func $call_imported))
        )
        "#,
    );
    let imported_module = compile(
        r#"
        (module
            (func $zero (result i32)
                i32.const 0
                i32.load
            )
            (memory $mem 1 1)
            (export "zero" (func $zero))
        )
    "#,
    );
    let answer = execute_0_deps(module, vec![("mod", imported_module)]);
    assert_eq!(answer.return_value, 42);
}

#[test]
fn global_read() {
    let module = compile(
        r#"
        (module
            (func $the_answer (result i32)
                global.get $glob
            )
            (global $glob i32 (i32.const 42))
            (export "main" (func $the_answer))
        )
    "#,
    );
    assert_eq!(execute_0(module), 42);
}

#[test]
fn global_write() {
    let module = compile(
        r#"
        (module
            (func $the_answer (result i32)
                i32.const 42
                global.set $glob
                global.get $glob
            )
            (global $glob (mut i32) (i32.const 0))
            (export "main" (func $the_answer))
        )
    "#,
    );
    assert_eq!(execute_0(module), 42);
}

#[test]
fn import_global() {
    let module = compile(
        r#"
        (module
            (type $t (func))
            (import "answer" "set_answer"
                (func $set_answer (type $t))
            )
            (import "answer" "the_answer"
                (global $the_answer i32)
            )
            (func $main (result i32)
                call $set_answer
                global.get $the_answer
            )
            (export "main" (func $main))
        )
        "#,
    );
    let imported_module = compile(
        r#"
        (module
            (func $set_answer
                i32.const 42
                global.set $answer
            )
            (global $answer (mut i32) (i32.const 0))
            (export "the_answer" (global $answer))
            (export "set_answer" (func $set_answer))
        )
    "#,
    );
    let answer = execute_0_deps(module, vec![("answer", imported_module)]);
    assert_eq!(answer.return_value, 42);
}

#[test]
/// The simplest possible program, compiled from Rust to Wasm.
fn the_answer_rust() {
    let module = compile(
        r#"
        (module
            (type (;0;) (func (result i32)))
            (func $answer (type 0) (result i32)
                i32.const 42
            )
            (table (;0;) 1 1 funcref)
            (memory (;0;) 16)
            (global (;0;) (mut i32) (i32.const 1048576))
            (global (;1;) i32 (i32.const 1048576))
            (global (;2;) i32 (i32.const 1048576))
            (export "memory" (memory 0))
            (export "main" (func $answer))
            (export "__data_end" (global 1))
            (export "__heap_base" (global 2)))
        "#,
    );
    assert_eq!(execute_0(module), 42);
}

// ———————————————————————————— Helper Functions ———————————————————————————— //

struct ExecutionResult<Area> {
    instance: Instance<Area>,
    return_value: i32,
}

fn compile(wat: &str) -> WasmModule {
    let bytecode = wat::parse_str(wat).unwrap();
    let mut comp = compiler::X86_64Compiler::new();
    comp.parse(&bytecode).unwrap();
    comp.compile().unwrap()
}

/// Execute a module, with no arguments passed to the main function.
fn execute_0(module: impl Module) -> i32 {
    let alloc = LibcAllocator::new();
    let mut instance = Instance::instantiate(&module, vec![], &alloc).unwrap();
    call_0(&mut instance)
}

/// Execute a module, with 2 arguments passed to the main function.
fn execute_2(module: impl Module, arg1: i32, arg2: i32) -> i32 {
    let alloc = LibcAllocator::new();

    let instance = Instance::instantiate(&module, vec![], &alloc).unwrap();

    unsafe {
        let fun = "main";
        let fun_ptr = instance.get_func_addr_by_name(fun).unwrap();

        let vmctx = instance.get_vmctx_ptr();
        let result: i32;
        asm!(
            "call {entry_point}",
            entry_point = in(reg) fun_ptr,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") vmctx,
            out("rax") result,
        );
        result
    }
}

/// Execute a module with dependencies, but with 0 arguments passed to the main function.
fn execute_0_deps(
    module: impl Module,
    dependencies: Vec<(&str, impl Module)>,
) -> ExecutionResult<impl MemoryArea> {
    let alloc = LibcAllocator::new();

    let dependencies = dependencies
        .into_iter()
        .map(|(name, module)| {
            (
                name,
                Instance::instantiate(&module, vec![], &alloc).unwrap(),
            )
        })
        .collect::<Vec<(&str, Instance<Arc<MMapArea>>)>>();
    let mut instance = Instance::instantiate(&module, dependencies, &alloc).unwrap();

    ExecutionResult {
        return_value: call_0(&mut instance),
        instance,
    }
}

/// Call the function "main" of an instance with 0 arguments, and return an i32 corresponding to
/// the value in rax (the return register).
fn call_0(instance: &mut Instance<impl MemoryArea>) -> i32 {
    unsafe {
        let fun = "main";
        let fun_ptr = instance.get_func_addr_by_name(fun).unwrap();

        let vmctx = instance.get_vmctx_ptr();
        let result: i32;
        asm!(
            "call {entry_point}",
            entry_point = in(reg) fun_ptr,
            in("rdi") vmctx,
            out("rax") result,
        );
        result
    }
}
