#![feature(asm)]
use std::fs;

use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_wasm::{translate_module, DummyEnvironment, ReturnMode};

mod env;

#[repr(align(0x1000))]
struct Page([u8; 0x1000]);

static mut TEXT_SECTION: Page = Page([0; 0x1000]);

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
    println!("{:#?}", wasm_env.info);

    let (_, fun) = wasm_env.info.fun_bodies.into_iter().next().unwrap().clone();
    let mut traps: Box<dyn cranelift_codegen::binemit::TrapSink> =
        Box::new(cranelift_codegen::binemit::NullTrapSink::default());
    let mut relocs: Box<dyn cranelift_codegen::binemit::RelocSink> =
        Box::new(cranelift_codegen::binemit::NullRelocSink::default());
    let mut stack_maps: Box<dyn cranelift_codegen::binemit::StackMapSink> =
        Box::new(cranelift_codegen::binemit::NullStackMapSink {});
    let mut ctx = cranelift_codegen::Context::for_function(fun);
    ctx.compile(&*target_isa).unwrap();
    let info = unsafe {
        ctx.emit_to_memory(
            TEXT_SECTION.0.as_mut_ptr(),
            &mut *relocs,
            &mut *traps,
            &mut *stack_maps,
        )
    };
    let len = info.total_size;
    println!("Code size: {:?}", len);

    // Great, now let's try to call that function by hand
    unsafe {
        let entry_point = TEXT_SECTION.0.as_ptr();
        let rip: u64;
        asm!(
            "leaq 0(%rip), {}", out(reg) rip, options(att_syntax)
        );
        println!("Current address: 0x{:x}", rip);
        println!("Target address:  {:p}", entry_point);
        let offset = entry_point as u64 - rip;
        println!("Offset:          0x{:x}", offset);

        let a: u32 = 2;
        let b: u32 = 3;
        let c: u32;
        asm!(
            "call "
        );
    }

    // let bytes = unsafe { std::slice::from_raw_parts(TEXT_SECTION.0.as_ptr(), len as usize) };
    // use std::io::Write;
    // let mut file = std::fs::File::create("a.out").unwrap();
    // file.write(bytes).unwrap();
}
