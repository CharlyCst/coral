#![feature(asm, allocator_api)]
use std::fs;

mod alloc;
mod collections;
mod compiler;
mod env;
mod instances;
mod modules;
mod traits;

use instances::Instance;
use traits::Compiler;

fn main() {
    println!("Kranelift");

    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        println!(
            "Usage: {} <wasm_file> [<import_1_name> <import_1_wasm_file> ...]",
            args[0]
        );
        return;
    } else {
        println!("Compiling: {}", &args[1]);
    }

    let alloc = alloc::LibcAllocator::new();

    // Iterate over the args 2 by 2, the first item is the module name, the second the file
    let imported_modules = args[2..]
        .windows(2)
        .step_by(2)
        .map(|arg| {
            let name = arg[0].clone();
            let path = &arg[1];
            eprintln!("Import: {} from {}", &name, path);
            (name, compile(path))
        })
        .collect::<Vec<(String, modules::SimpleModule)>>();
    let imported_instances = imported_modules
        .iter()
        .map(|(name, module)| {
            (
                name.as_str(),
                Instance::instantiate(module, vec![], &alloc).unwrap(),
            )
        })
        .collect::<Vec<(&str, Instance<alloc::LibcAllocator>)>>();

    let module = compile(&args[1]);
    let instance = Instance::instantiate(&module, imported_instances, &alloc).unwrap();

    // Great, now let's try to call that function by hand
    unsafe {
        let fun = "double_add";
        let fun_ptr = instance.get_func_addr_from_name(fun).unwrap();
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

fn compile(file: &str) -> modules::SimpleModule {
    let bytecode = match fs::read(file) {
        Ok(b) => b,
        Err(err) => {
            println!("File Error: {}", err);
            std::process::exit(1);
        }
    };
    let mut comp = compiler::X86_64Compiler::new();
    comp.parse(&bytecode).unwrap();
    comp.compile().unwrap()
}
