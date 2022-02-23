use std::collections::HashMap;

use cranelift_codegen::binemit::{Addend, CodeOffset, Reloc as RelocKind, RelocSink, TrapSink};
use cranelift_codegen::entity::{entity_impl, PrimaryMap};
use cranelift_codegen::ir;
use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_wasm::{translate_module, ModuleTranslationState};

use crate::env;
use crate::traits::{self, CompilerError, HeapKind, ModuleError};

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
    type Module = Module;

    fn parse(&mut self, wasm_bytecode: &[u8]) -> traits::CompilerResult<()> {
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

    fn compile(self) -> traits::CompilerResult<Module> {
        let mut code = Vec::new();
        let mut mod_info = ModuleInfo::new();
        let mut relocs = RelocationHandler::new();
        let mut traps: Box<dyn cranelift_codegen::binemit::TrapSink> = Box::new(TrapHandler::new());
        // TODO: handle stack maps
        let mut stack_maps: Box<dyn cranelift_codegen::binemit::StackMapSink> =
            Box::new(cranelift_codegen::binemit::NullStackMapSink {});

        for (_, (fun, fun_idx)) in self.module.info.fun_bodies.into_iter() {
            // Compile and emit to memory
            let name: Name = (&fun.name).into();
            let offset = code.len() as u32;
            let fun_info = &self.module.info.funs[fun_idx];
            let mut ctx = cranelift_codegen::Context::for_function(fun);
            mod_info.register_func(name, &fun_info.export_names, offset);

            relocs.set_offset(offset);
            let mut relocs = relocs.as_dyn();
            ctx.compile_and_emit(
                &*self.target_isa,
                &mut code,
                &mut *relocs,
                &mut *traps,
                &mut *stack_maps,
            )
            .map_err(|err| {
                eprintln!("Err: {:?}", err);
                CompilerError::FailedToCompile
            })?; // TODO: better error handling
        }

        for memory in self.module.info.memories {
            mod_info.register_heap(memory.minimum as u32, memory.maximum.map(|x| x as u32));
        }

        Ok(Module::new(mod_info, code, relocs.relocs))
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

impl traits::Allocator for LibcAllocator {
    fn alloc_code(&mut self, code_size: u32) -> *mut u8 {
        let code_size = code_size as usize;
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

    fn alloc_heap(
        &mut self,
        min_size: u32,
        _max_size: Option<u32>,
        _kind: traits::HeapKind,
    ) -> *mut u8 {
        let ptr = unsafe {
            libc::mmap(
                0 as *mut libc::c_void,
                PAGE_SIZE * min_size as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr == 0 as *mut libc::c_void {
            panic!("Failled mmap for heap allocation");
        }
        ptr as *mut u8
    }
}

// ————————————————————————————————— Module ————————————————————————————————— //

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FuncIndex(u32);
entity_impl!(FuncIndex);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HeapIndex(u32);
entity_impl!(HeapIndex);

pub enum ModuleItem {
    Func(FuncIndex),

    // TODO: handle heap export (i.e. named heaps)
    #[allow(unused)]
    Heap(HeapIndex),
}

pub struct FunctionInfo {
    pub offset: u32,
    // TODO: add signature
}

pub struct HeapInfo {
    pub min_size: u32,
    pub max_size: Option<u32>,
    pub kind: HeapKind,
}

pub struct ModuleInfo {
    exported_names: HashMap<String, Name>,
    items: HashMap<Name, ModuleItem>,
    funs: PrimaryMap<FuncIndex, FunctionInfo>,
    heaps: PrimaryMap<HeapIndex, HeapInfo>,
}

impl ModuleInfo {
    pub fn new() -> Self {
        Self {
            exported_names: HashMap::new(),
            items: HashMap::new(),
            funs: PrimaryMap::new(),
            heaps: PrimaryMap::new(),
        }
    }

    fn register_func(&mut self, name: Name, exported_names: &Vec<String>, offset: u32) {
        let func_info = FunctionInfo { offset };
        let idx = self.funs.push(func_info);
        self.items.insert(name, ModuleItem::Func(idx));

        // Export the function, if required
        for exported_name in exported_names {
            self.exported_names.insert(exported_name.to_owned(), name);
        }
    }

    fn register_heap(&mut self, min_size: u32, max_size: Option<u32>) {
        let kind = match max_size {
            Some(max_size) => HeapKind::Static { max_size },
            None => HeapKind::Dynamic,
        };
        self.heaps.push(HeapInfo {
            min_size,
            max_size,
            kind,
        });
    }

    pub fn _get_function<'a, 'b>(&'a self, symbol: &'b str) -> Option<&'a FunctionInfo> {
        let name = self.exported_names.get(symbol)?;
        self.get_func_by_name(*name)
    }

    pub fn get_func_by_name(&self, name: Name) -> Option<&FunctionInfo> {
        match self.items.get(&name) {
            Some(&ModuleItem::Func(idx)) => Some(&self.funs[idx]),
            _ => None,
        }
    }
}

pub struct Module {
    pub info: ModuleInfo,
    pub code: Vec<u8>,
    relocs: Vec<Reloc>, // TODO: resolve offset at compile time
}

impl Module {
    pub fn new(info: ModuleInfo, code: Vec<u8>, relocs: Vec<Reloc>) -> Self {
        Self { info, code, relocs }
    }

    /// Apply relocations to code inside a buffer.
    ///
    /// ## Safety
    ///
    /// The caller must ensure that all the relocation positions are valid.
    unsafe fn apply_relocs(&self, code: &mut [u8], code_addr: i64) -> traits::ModuleResult<()> {
        self._apply_relocs_inner(code, code_addr)
    }

    /// Internal function, should only be called from `apply_relocs`.
    fn _apply_relocs_inner(&self, code: &mut [u8], code_addr: i64) -> traits::ModuleResult<()> {
        for reloc in &self.relocs {
            let addend = reloc.addend;
            let value = if let Some(func) = self.info.get_func_by_name(reloc.name) {
                code_addr + func.offset as i64
            } else {
                // We dont handle external symbols for now
                return Err(ModuleError::FailedToInstantiate);
            };
            let offset = reloc.offset as usize;
            match reloc.kind {
                RelocKind::Abs4 => todo!(),
                RelocKind::Abs8 => {
                    let final_value = value + addend;
                    code[offset..][..8].copy_from_slice(&final_value.to_le_bytes());
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

    fn build_datastructures<Alloc>(&self, alloc: &mut Alloc) -> (VMContext, Vec<Heap>)
    where
        Alloc: traits::Allocator,
    {
        let mut vmctx = Vec::new();
        let mut heaps = Vec::new();
        for (_, heap) in &self.info.heaps {
            let heap_ptr = alloc.alloc_heap(heap.min_size, heap.max_size, heap.kind);
            heaps.push(Heap { addr: heap_ptr });
            vmctx.push(heap_ptr);
        }
        (vmctx, heaps)
    }
}

impl traits::Module for Module {
    type Instance = Instance;

    fn instantiate<Alloc>(&self, alloc: &mut Alloc) -> traits::ModuleResult<Self::Instance>
    where
        Alloc: traits::Allocator,
    {
        let code_size = self.code.len();
        let code_ptr = alloc.alloc_code(code_size as u32);

        // SAFETY: We rely on the correctness of the allocator that must return a pointer to an
        // unused memory region of the appropriate size.
        unsafe {
            let code = core::slice::from_raw_parts_mut(code_ptr, code_size);
            code.copy_from_slice(&self.code);
            self.apply_relocs(code, code_ptr as i64)?;
        };

        let mut instance = Instance::new();

        // Collect exported symbols
        let info = &self.info;
        for (exported_name, name) in &info.exported_names {
            let item = &info.items[&name];
            let symbol = match item {
                ModuleItem::Func(idx) => {
                    let func = &info.funs[*idx];
                    let addr = code_ptr.wrapping_add(func.offset as usize);
                    Symbol::Function { addr }
                }
                ModuleItem::Heap(_) => todo!("Handle memory export"),
            };
            instance.symbols.insert(exported_name.to_owned(), symbol);
        }

        // Instantiate data structures (e.g. heaps, tables...)
        let (vmctx, heaps) = self.build_datastructures(alloc);
        instance.vmctx = vmctx;
        instance.heaps = heaps;

        Ok(instance)
    }
}

// ———————————————————————————————— Instance ———————————————————————————————— //

pub enum Symbol {
    Function { addr: *const u8 },
}

impl Symbol {
    pub fn addr(&self) -> *const u8 {
        match self {
            Symbol::Function { addr } => *addr,
        }
    }
}

type VMContext = Vec<*mut u8>;

struct Heap {
    // TODO: handle heap in a cleaner way (i.e. use slices).
    #[allow(unused)]
    addr: *mut u8,
}

pub struct Instance {
    /// A map of all exported symbols.
    symbols: HashMap<String, Symbol>,
    /// The VM Context, contains pointers to various structures, such as heaps and tables.
    ///
    /// For now, only 8 bytes pointers are handled.
    vmctx: VMContext,
    /// The heaps of the instance.
    heaps: Vec<Heap>,
}

impl Instance {
    pub fn new() -> Self {
        Self {
            symbols: HashMap::new(),
            vmctx: Vec::new(),
            heaps: Vec::new(),
        }
    }

    pub fn get<'a, 'b>(&'a self, symbol: &'b str) -> Option<&'a Symbol> {
        self.symbols.get(symbol)
    }

    pub fn get_vmctx(&self) -> &VMContext {
        &self.vmctx
    }
}

// —————————————————————————————— Trap Handler —————————————————————————————— //

pub struct TrapHandler {}

impl TrapHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl TrapSink for TrapHandler {
    fn trap(&mut self, _offset: CodeOffset, _loc: ir::SourceLoc, _code: ir::TrapCode) {
        // NOTE: can be enabled for debugging
        // eprintln!("Trap at 0x{:x} - loc {:?} - code {:?}", _offset, _loc, _code);
    }
}

// ——————————————————————————— Relocation Handler ——————————————————————————— //

pub struct Reloc {
    offset: u32,
    kind: RelocKind,
    name: Name,
    addend: Addend,
}

pub struct RelocationHandler {
    relocs: Vec<Reloc>,
    func_offset: u32,
}

pub struct RelocationProxy<'handler> {
    handler: &'handler mut RelocationHandler,
}

impl RelocationHandler {
    pub fn new() -> Self {
        Self {
            relocs: Vec::new(),
            func_offset: 0,
        }
    }

    pub fn set_offset(&mut self, offset: u32) {
        self.func_offset = offset;
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
            offset: self.func_offset + offset as u32,
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
