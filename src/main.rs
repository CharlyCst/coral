use std::fs;

use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_wasm::{translate_module, DummyEnvironment, ReturnMode};

mod env;

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

    let target_isa = isa::lookup_by_name("x86_64").unwrap();
    let target_settings = settings::builder();
    let flags = settings::Flags::new(target_settings);
    println!("Opt level: {:?}", flags.opt_level());
    let target_isa = target_isa.finish(flags);
    let produce_debuginfo = false;
    let mut _wasm_env = DummyEnvironment::new(
        target_isa.frontend_config(),
        ReturnMode::NormalReturns,
        produce_debuginfo,
    );
    let mut wasm_env = env::ModuleEnvironment::new(target_isa.frontend_config());

    let translation_result = translate_module(&bytecode, &mut wasm_env);
    let module_metadata = match translation_result {
        Ok(module) => module,
        Err(err) => {
            println!("Compilation Error: {:?}", &err);
            return;
        }
    };

    println!("Module: {:#?}", module_metadata);
}
