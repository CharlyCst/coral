use std::collections::HashMap;

use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_wasm::{translate_module, ModuleTranslationState};

use crate::env;
use crate::traits;

pub struct X86_64Compiler {
    module: env::ModuleEnvironment,
    module_metadata: Option<ModuleTranslationState>,
    target_isa: Box<dyn isa::TargetIsa>,
}

impl X86_64Compiler {
    pub fn new() -> Self {
        let flags = settings::Flags::new(settings::builder());
        let target_isa = isa::lookup_by_name("x86_64").unwrap().finish(flags);
        let module = env::ModuleEnvironment::new(target_isa.frontend_config());

        Self {
            module,
            target_isa,
            module_metadata: None,
        }
    }
}

impl traits::Compiler for X86_64Compiler {
    type Module = ModuleInfo;

    fn parse(&mut self, wasm_bytecode: &[u8]) -> traits::CompilerResults<()> {
        let translation_result = translate_module(wasm_bytecode, &mut self.module);
        match translation_result {
            Ok(module) => {
                self.module_metadata = Some(module);
                Ok(())
            }
            Err(err) => {
                println!("Compilation Error: {:?}", &err);
                Err(traits::CompilerError::FailedToParse)
            }
        }
    }

    fn compile<Alloc>(self, alloc: &mut Alloc) -> traits::CompilerResults<ModuleInfo>
    where
        Alloc: traits::ModuleAllocator,
    {
        let mut mod_info = ModuleInfo::new();

        // TODO: handle those
        let mut traps: Box<dyn cranelift_codegen::binemit::TrapSink> =
            Box::new(cranelift_codegen::binemit::NullTrapSink::default());
        let mut relocs: Box<dyn cranelift_codegen::binemit::RelocSink> =
            Box::new(cranelift_codegen::binemit::NullRelocSink::default());
        let mut stack_maps: Box<dyn cranelift_codegen::binemit::StackMapSink> =
            Box::new(cranelift_codegen::binemit::NullStackMapSink {});

        for (_fun_idx, fun) in self.module.info.fun_bodies.into_iter() {
            let mut ctx = cranelift_codegen::Context::for_function(fun);

            let code_info = ctx.compile(&*self.target_isa).unwrap();
            let code_size = code_info.total_size as usize;
            let code_ptr = alloc.alloc_code(code_size);
            let _info = unsafe {
                ctx.emit_to_memory(code_ptr, &mut *relocs, &mut *traps, &mut *stack_maps)
            };

            // TODO: only append if the function is marked as exposed
            mod_info
                .funs
                .insert(String::from("add"), FunctionInfo { ptr: code_ptr });
        }

        Ok(mod_info)
    }
}

pub struct LibcAllocator {
    next: *mut u8,
    capacity: usize,
    chunks: Vec<*mut u8>,
}

const PAGE_SIZE: usize = 0x1000;

impl LibcAllocator {
    pub fn new() -> Self {
        Self {
            next: 0 as *mut u8,
            capacity: 0,
            chunks: Vec::new(),
        }
    }

    /// Increase the current capacity by allocating a new block
    fn increase(&mut self) {
        unsafe {
            let ptr = libc::mmap(
                0 as *mut libc::c_void,
                PAGE_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            if ptr == 0 as *mut libc::c_void {
                panic!("Failled mmap");
            }

            self.next = ptr as *mut u8;
            self.capacity = PAGE_SIZE;
            self.chunks.push(ptr as *mut u8);
        }
    }
}

impl traits::ModuleAllocator for LibcAllocator {
    fn alloc_code(&mut self, code_size: usize) -> *mut u8 {
        if self.capacity < code_size {
            self.increase();
            if self.capacity < code_size {
                panic!("Code exceed the maximum capacity");
            }
        }

        let ptr = self.next;
        self.capacity -= code_size;
        self.next = self.next.wrapping_offset(code_size as isize);

        ptr
    }

    fn alloc_memory(&mut self) {
        todo!()
    }

    fn terminate(self) {
        for ptr in self.chunks {
            unsafe {
                let ok = libc::mprotect(
                    ptr as *mut libc::c_void,
                    PAGE_SIZE,
                    libc::PROT_READ | libc::PROT_EXEC,
                );
                if ok != 0 {
                    panic!(
                        "Could not set memory executable: errno {}",
                        *libc::__errno_location()
                    );
                }
            }
        }
    }
}

pub struct FunctionInfo {
    pub ptr: *const u8,
    // TODO: add signature
}

pub struct ModuleInfo {
    funs: HashMap<String, FunctionInfo>,
}

impl ModuleInfo {
    pub fn new() -> Self {
        Self {
            funs: HashMap::new(),
        }
    }

    pub fn get_function<'a, 'b>(&'a self, name: &'b str) -> Option<&'a FunctionInfo> {
        self.funs.get(name)
    }
}
