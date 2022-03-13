use cranelift_codegen::binemit::{Addend, CodeOffset, Reloc as RelocKind, RelocSink, TrapSink};
use cranelift_codegen::ir;
use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_wasm::{translate_module, ModuleTranslationState};

use crate::collections::{EntityRef, FrozenMap, PrimaryMap, SecondaryMap};
use crate::env;
use crate::modules::{ModuleInfo, SimpleModule};
use crate::traits::ItemRef;
use crate::traits::{Compiler, CompilerError, CompilerResult};
use crate::traits::{FuncIndex, FuncInfo, Reloc};
use crate::traits::{HeapInfo, HeapKind};

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

impl Compiler for X86_64Compiler {
    type Module = SimpleModule;

    fn parse(&mut self, wasm_bytecode: &[u8]) -> CompilerResult<()> {
        let translation_result = translate_module(wasm_bytecode, &mut self.module);
        match translation_result {
            Ok(module) => {
                self.module_metadata = Some(module);
                Ok(())
            }
            Err(err) => {
                println!("Compilation Error: {:?}", &err);
                Err(CompilerError::FailedToParse)
            }
        }
    }

    fn compile(self) -> CompilerResult<SimpleModule> {
        let module_info = self.module.info;
        let mut imported_funcs = module_info.imported_funcs;
        let modules = FrozenMap::freeze(module_info.modules);

        // Build the functions info
        let mut funcs = PrimaryMap::with_capacity(module_info.funcs.len());
        let mut funcs_names = SecondaryMap::new();
        for (func_idx, func_names) in module_info.funcs {
            // We move out with `take` to avoid cloning the name
            let func = if let Some(import_info) = imported_funcs[func_idx].take() {
                FuncInfo::Imported {
                    module: import_info.module,
                    name: import_info.name,
                }
            } else {
                FuncInfo::Owned {
                    // WARNING: The offset **must** be set once known!
                    offset: 0,
                }
            };
            let func_idx = funcs.push(func);
            funcs_names[func_idx] = func_names.export_names;
        }
        let funcs = FrozenMap::freeze(funcs);

        // Build the heaps info
        let mut heaps = PrimaryMap::new();
        for heap in module_info.heaps {
            let min_size = heap.minimum as u32;
            let heap = match heap.maximum {
                Some(max_size) => HeapInfo {
                    min_size,
                    kind: HeapKind::Static {
                        max_size: max_size as u32,
                    },
                },
                None => HeapInfo {
                    min_size,
                    kind: HeapKind::Dynamic,
                },
            };
            heaps.push(heap);
        }
        let heaps = FrozenMap::freeze(heaps);

        let mut mod_info = ModuleInfo::new(funcs, heaps, modules);
        for (func_idx, names) in funcs_names.iter() {
            mod_info.export_func(func_idx, names);
        }

        let mut code = Vec::new();
        let mut relocs = RelocationHandler::new();
        let mut traps: Box<dyn cranelift_codegen::binemit::TrapSink> = Box::new(TrapHandler::new());
        // TODO: handle stack maps
        let mut stack_maps: Box<dyn cranelift_codegen::binemit::StackMapSink> =
            Box::new(cranelift_codegen::binemit::NullStackMapSink {});

        // Compile and emit to memory
        for (_, (func, func_idx)) in module_info.func_bodies.into_iter() {
            let offset = code.len() as u32;
            // transmute index from cranelift_wasm to internal
            let func_idx = FuncIndex::new(func_idx.index());
            mod_info.set_func_offset(func_idx, offset);
            // let fun_info = &self.module.info.funcs[func_idx];
            // mod_info.register_func(&fun_info.export_names, offset);
            let mut ctx = cranelift_codegen::Context::for_function(func);

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

        Ok(SimpleModule::new(mod_info, code, relocs.relocs))
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

    /// Translate an ir::ExternalName to an item reference.
    pub fn translate(&self, name: &ir::ExternalName) -> ItemRef {
        match name {
            ir::ExternalName::User { index, .. } => {
                // WARNING: we are relying on the fact that ir::ExternalName are attributed in the
                // **exact** same order as FuncIndex. This is a contract between the
                // ModuleEnvironment and the Compiler.
                ItemRef::Func(FuncIndex::new(*index as usize))
            }
            _ => panic!("Unexpected name!"),
        }
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
            item: self.translate(name),
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
