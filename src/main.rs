#![feature(asm)]
use std::fs;

mod compiler;
mod env;
mod traits;

use traits::Allocator;
use traits::Compiler;
use traits::Module;

fn main() {
    println!("Kranelift");

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        println!("Usage: {} <wasm_file>", args[0]);
        return;
    } else {
        println!("Compiling: {}", &args[1]);
    }

    let bytecode = match fs::read(&args[1]) {
        Ok(b) => b,
        Err(err) => {
            println!("File Error: {}", err);
            return;
        }
    };

    let mut comp = compiler::X86_64Compiler::new();
    let mut alloc = compiler::LibcAllocator::new();

    comp.parse(&bytecode).unwrap();
    let module = comp.compile().unwrap();
    let instance = module.instantiate(&mut alloc).unwrap();
    alloc.terminate();

    // Great, now let's try to call that function by hand
    unsafe {
        let fun = "double_add";
        let fun_ptr = instance.get(fun).unwrap().addr();
        println!("Fun addr: {:p}", fun_ptr);

        let a: u32 = 2;
        let b: u32 = 3;
        let vmctx = instance.get_vmctx().as_ptr();
        let c: u32;
        asm!(
            "call {entry_point}",
            entry_point = in(reg) fun_ptr,
            in("rdi") a,
            in("rsi") b,
            in("rdx") vmctx,
            out("rax") c,
        );
        println!("{}({}, {}) = {}", fun, a, b, c);
    }
}
