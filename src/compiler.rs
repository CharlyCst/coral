use std::collections::HashMap;

use cranelift_codegen::binemit::{Addend, CodeOffset, Reloc as RelocKind, RelocSink};
use cranelift_codegen::ir;
use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_wasm::{translate_module, ModuleTranslationState};

use crate::env;
use crate::traits;

// ————————————————————————————————— Utils —————————————————————————————————— //

/// An name that can be used to index module items.
#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct Name {
    namespace: u32,
    index: u32,
}

impl From<ir::ExternalName> for Name {
    fn from(name: ir::ExternalName) -> Self {
        (&name).into()
    }
}

impl From<&ir::ExternalName> for Name {
    fn from(name: &ir::ExternalName) -> Self {
        match name {
            ir::ExternalName::User { namespace, index } => Self {
                namespace: *namespace,
                index: *index,
            },
            ir::ExternalName::TestCase { .. } => todo!(),
            ir::ExternalName::LibCall(_) => todo!(),
        }
    }
}

// ———————————————————————————————— Compiler ———————————————————————————————— //

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

        let mut relocs = RelocationHandler::new();
        // TODO: handle those
        let mut traps: Box<dyn cranelift_codegen::binemit::TrapSink> =
            Box::new(cranelift_codegen::binemit::NullTrapSink::default());
        let mut stack_maps: Box<dyn cranelift_codegen::binemit::StackMapSink> =
            Box::new(cranelift_codegen::binemit::NullStackMapSink {});

        for (_, (fun, fun_idx)) in self.module.info.fun_bodies.into_iter() {
            // Compile and emit to memory
            let name: Name = (&fun.name).into();
            let mut ctx = cranelift_codegen::Context::for_function(fun);
            let code_info = ctx.compile(&*self.target_isa).unwrap();
            let code_size = code_info.total_size as usize;
            let code_ptr = alloc.alloc_code(code_size);
            relocs.set_base_addr(code_ptr as u64);
            relocs.register_item(name, code_ptr as u64);
            // SAFETY: the code pointer must point to a valid writable regions with enough capacity
            // to contain the whole function body.
            let _info = unsafe {
                ctx.emit_to_memory(
                    code_ptr,
                    &mut *relocs.as_dyn(),
                    &mut *traps,
                    &mut *stack_maps,
                )
            };

            // Export the function, if required
            let fun_info = &self.module.info.funs[fun_idx];
            for name in &fun_info.export_names {
                mod_info
                    .funs
                    .insert(name.to_owned(), FunctionInfo { ptr: code_ptr });
            }
        }

        // Apply relocations
        unsafe {
            relocs.apply_relocs().unwrap();
        }

        Ok(mod_info)
    }
}

// ———————————————————————————— Module Allocator ———————————————————————————— //

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

// ——————————————————————————— Relocation Handler ——————————————————————————— //

struct Reloc {
    addr: u64,
    kind: RelocKind,
    name: Name,
    addend: Addend,
}

pub struct RelocationHandler {
    relocs: Vec<Reloc>,
    // Addresses of various items in the current module
    module_items: HashMap<Name, u64>,
    base_addr: u64,
}

pub struct RelocationProxy<'handler> {
    handler: &'handler mut RelocationHandler,
}

impl RelocationHandler {
    pub fn new() -> Self {
        Self {
            relocs: Vec::new(),
            module_items: HashMap::new(),
            base_addr: 0,
        }
    }

    /// Set the base address of the relocations.
    /// This functions must be called with the code address before emitting relocation for a
    /// function.
    pub fn set_base_addr(&mut self, base_addr: u64) {
        self.base_addr = base_addr;
    }

    pub fn register_item(&mut self, name: Name, addr: u64) {
        self.module_items.insert(name.into(), addr);
    }

    /// Apply all the relocations previously collected.
    ///
    /// ## Safety
    ///
    /// This function writes relocations directly to memory, the caller must ensure that all the
    /// relocation positions are valid, in particular that the base address has been set correctly
    /// using the [`set_base_addr`] method.
    pub unsafe fn apply_relocs(self) -> Result<(), ()> {
        self.apply_relocs_inner()
    }

    fn apply_relocs_inner(self) -> Result<(), ()> {
        for reloc in &self.relocs {
            let addend = reloc.addend;
            let value = self.get_addr(reloc.name).ok_or(())?;
            match reloc.kind {
                RelocKind::Abs4 => todo!(),
                RelocKind::Abs8 => {
                    let ptr = reloc.addr as *mut i64;
                    unsafe {
                        *ptr = value as i64 + addend;
                    }
                }
                RelocKind::X86PCRel4 => todo!(),
                RelocKind::X86CallPCRel4 => todo!(),
                RelocKind::X86CallPLTRel4 => todo!(),
                RelocKind::X86GOTPCRel4 => todo!(),
                RelocKind::Arm32Call => todo!(),
                RelocKind::Arm64Call => todo!(),
                RelocKind::S390xPCRel32Dbl => todo!(),
                RelocKind::ElfX86_64TlsGd => todo!(),
                RelocKind::MachOX86_64Tlv => todo!(),
                RelocKind::Aarch64TlsGdAdrPage21 => todo!(),
                RelocKind::Aarch64TlsGdAddLo12Nc => todo!(),
            }
        }

        Ok(())
    }

    fn get_addr(&self, name: Name) -> Option<u64> {
        self.module_items.get(&name).cloned()
    }

    /// Wrap the relocation handler into a dynamic object.
    pub fn as_dyn<'a>(&'a mut self) -> Box<dyn RelocSink + 'a> {
        // TODO: remove allocation
        Box::new(RelocationProxy { handler: self })
    }
}

impl RelocSink for RelocationHandler {
    fn reloc_external(
        &mut self,
        offset: CodeOffset,
        _: ir::SourceLoc,
        kind: RelocKind,
        name: &ir::ExternalName,
        addend: Addend,
    ) {
        println!(
            "Reloc: offset 0x{:x} - kind {:?} - name {:?} - addend 0x{:x}",
            offset, kind, name, addend
        );

        self.relocs.push(Reloc {
            addr: self.base_addr + offset as u64,
            name: name.into(),
            kind,
            addend,
        });
    }
}

impl<'handler> RelocSink for RelocationProxy<'handler> {
    fn reloc_external(
        &mut self,
        offset: CodeOffset,
        source_loc: ir::SourceLoc,
        kind: RelocKind,
        name: &ir::ExternalName,
        addend: Addend,
    ) {
        self.handler
            .reloc_external(offset, source_loc, kind, name, addend)
    }
}
