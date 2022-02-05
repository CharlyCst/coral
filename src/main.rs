#![feature(asm)]
use std::fs;

mod compiler;
mod env;
mod traits;

use traits::Compiler;
use traits::ModuleAllocator;

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
    let module = comp.compile(&mut alloc).unwrap();
    alloc.terminate();

    // Great, now let's try to call that function by hand
    unsafe {
        let fun = module.get_function("add").unwrap();
        println!("Fun addr: {:p}", fun.ptr);

        let a: u32 = 2;
        let b: u32 = 3;
        let c: u32;
        let addr: u64 = fun.ptr as u64;
        asm!(
            "call {entry_point}",
            entry_point = in(reg) addr,
            in("rdi") a,
            in("rsi") b,
            out("rax") c,
        );
        println!("{} + {} = {}", a, b, c);
    }
}
