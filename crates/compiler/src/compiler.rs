use alloc::boxed::Box;
use alloc::vec::Vec;

use cranelift_codegen::binemit::{
    Addend, CodeOffset, Reloc as CraneliftRelocKind, RelocSink, TrapSink,
};
use cranelift_codegen::ir;
use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_wasm::{translate_module, GlobalInit, ModuleTranslationState};

use ocean_collections::{EntityRef, FrozenMap, PrimaryMap, SecondaryMap};
use ocean_wasm::{Compiler, CompilerError, CompilerResult, GlobInit};
use ocean_wasm::{FuncIndex, FuncInfo, Reloc, RelocKind};
use ocean_wasm::{GlobInfo, ItemRef};
use ocean_wasm::{HeapInfo, HeapKind};
use ocean_wasm::{ModuleInfo, SimpleModule};

use crate::env;

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
            Err(_err) => Err(CompilerError::FailedToParse),
        }
    }

    fn compile(self) -> CompilerResult<SimpleModule> {
        let module_info = self.module.info;
        let mut imported_funcs = module_info.imported_funcs;
        let mut imported_heaps = module_info.imported_heaps;
        let mut imported_globs = module_info.imported_globs;
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
        let mut heaps_names = SecondaryMap::new();
        for (heap_idx, heap) in module_info.heaps {
            // TODO: handle imported heaps
            let names = heap.export_names;
            let heap = heap.entity;
            let min_size = heap.minimum as u32;
            let heap = if let Some(import_info) = imported_heaps[heap_idx].take() {
                HeapInfo::Imported {
                    module: import_info.module,
                    name: import_info.name,
                }
            } else {
                match heap.maximum {
                    Some(max_size) => HeapInfo::Owned {
                        min_size,
                        kind: HeapKind::Static {
                            max_size: max_size as u32,
                        },
                    },
                    None => HeapInfo::Owned {
                        min_size,
                        kind: HeapKind::Dynamic,
                    },
                }
            };
            let heap_idx = heaps.push(heap);
            heaps_names[heap_idx] = names;
        }
        let heaps = FrozenMap::freeze(heaps);

        // Build the globals info
        let mut globs = PrimaryMap::new();
        let mut globs_names = SecondaryMap::new();
        for (glob_idx, glob) in module_info.globs {
            let names = glob.export_names;
            let glob = glob.entity;
            // We move out with `take` to avoid cloning the name
            // TODO: handle imported globals
            let glob = if let Some(import_info) = imported_globs[glob_idx].take() {
                GlobInfo::Imported {
                    module: import_info.module,
                    name: import_info.name,
                }
            } else {
                let init = convert_glob_init(glob.initializer);
                GlobInfo::Owned { init }
            };
            let glob_idx = globs.push(glob);
            globs_names[glob_idx] = names;
        }
        let globs = FrozenMap::freeze(globs);

        let mut mod_info = ModuleInfo::new(funcs, heaps, globs, modules);
        for (func_idx, names) in funcs_names.iter() {
            mod_info.export_func(func_idx, names);
        }
        for (heap_idx, names) in heaps_names.iter() {
            mod_info.export_heap(heap_idx, names);
        }
        for (glob_idx, names) in globs_names.iter() {
            mod_info.export_glob(glob_idx, names);
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
            .map_err(|_err| CompilerError::FailedToCompile)?; // TODO: better error handling
        }

        Ok(SimpleModule::new(mod_info, code, relocs.relocs))
    }
}

fn convert_glob_init(init: GlobalInit) -> GlobInit {
    match init {
        GlobalInit::I32Const(x) => GlobInit::I32(x),
        GlobalInit::I64Const(x) => GlobInit::I64(x),
        // NOTE: Can we get rid of the unsafe for the conversion?
        GlobalInit::F32Const(x) => unsafe { GlobInit::F32(core::mem::transmute(x)) },
        GlobalInit::F64Const(x) => unsafe { GlobInit::F64(core::mem::transmute(x)) },
        GlobalInit::V128Const(_) => todo!(),
        GlobalInit::GetGlobal(_) => todo!(),
        GlobalInit::RefNullConst => todo!(),
        GlobalInit::RefFunc(_) => todo!(),
        // Should never happen, we handle imports in a separate case
        GlobalInit::Import => panic!(),
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
        kind: CraneliftRelocKind,
        name: &ir::ExternalName,
        addend: Addend,
    ) {
        let kind = match kind {
            CraneliftRelocKind::Abs4 => RelocKind::Abs4,
            CraneliftRelocKind::Abs8 => RelocKind::Abs8,
            CraneliftRelocKind::X86PCRel4 => RelocKind::X86PCRel4,
            CraneliftRelocKind::X86CallPCRel4 => RelocKind::X86CallPCRel4,
            CraneliftRelocKind::X86CallPLTRel4 => RelocKind::X86CallPLTRel4,
            CraneliftRelocKind::X86GOTPCRel4 => RelocKind::X86GOTPCRel4,
            CraneliftRelocKind::Arm32Call => RelocKind::Arm32Call,
            CraneliftRelocKind::Arm64Call => RelocKind::Arm64Call,
            CraneliftRelocKind::S390xPCRel32Dbl => RelocKind::S390xPCRel32Dbl,
            CraneliftRelocKind::ElfX86_64TlsGd => RelocKind::ElfX86_64TlsGd,
            CraneliftRelocKind::MachOX86_64Tlv => RelocKind::MachOX86_64Tlv,
            CraneliftRelocKind::Aarch64TlsGdAdrPage21 => RelocKind::Aarch64TlsGdAdrPage21,
            CraneliftRelocKind::Aarch64TlsGdAddLo12Nc => RelocKind::Aarch64TlsGdAddLo12Nc,
        };

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
        kind: CraneliftRelocKind,
        name: &ir::ExternalName,
        addend: Addend,
    ) {
        self.handler
            .reloc_external(offset, source_loc, kind, name, addend)
    }
}