use wat;

use crate::alloc;
use crate::compiler;
use crate::instances::Instance;
use crate::modules;
use crate::traits::{Compiler, Module};

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
    assert_eq!(answer, 42);
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
    assert_eq!(answer, 42);
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


// ———————————————————————————— Helper Functions ———————————————————————————— //

fn compile(wat: &str) -> modules::SimpleModule {
    let bytecode = wat::parse_str(wat).unwrap();
    let mut comp = compiler::X86_64Compiler::new();
    comp.parse(&bytecode).unwrap();
    comp.compile().unwrap()
}

/// Execute a module, with no arguments passed to the main function.
fn execute_0(module: impl Module) -> i32 {
    let alloc = alloc::LibcAllocator::new();

    let instance = Instance::instantiate(&module, vec![], &alloc).unwrap();

    unsafe {
        let fun = "main";
        let fun_ptr = instance.get_func_addr_from_name(fun).unwrap();

        let vmctx = instance.get_vmctx().as_ptr();
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

/// Execute a module, with 2 arguments passed to the main function.
fn execute_2(module: impl Module, arg1: i32, arg2: i32) -> i32 {
    let alloc = alloc::LibcAllocator::new();

    let instance = Instance::instantiate(&module, vec![], &alloc).unwrap();

    unsafe {
        let fun = "main";
        let fun_ptr = instance.get_func_addr_from_name(fun).unwrap();

        let vmctx = instance.get_vmctx().as_ptr();
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
fn execute_0_deps(module: impl Module, dependencies: Vec<(&str, impl Module)>) -> i32 {
    let alloc = alloc::LibcAllocator::new();

    let dependencies = dependencies
        .into_iter()
        .map(|(name, module)| {
            (
                name,
                Instance::instantiate(&module, vec![], &alloc).unwrap(),
            )
        })
        .collect::<Vec<(&str, Instance<alloc::LibcAllocator>)>>();
    let instance = Instance::instantiate(&module, dependencies, &alloc).unwrap();

    unsafe {
        let fun = "main";
        let fun_ptr = instance.get_func_addr_from_name(fun).unwrap();

        let vmctx = instance.get_vmctx().as_ptr();
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
