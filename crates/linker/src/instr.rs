//! Instructions
//!
//! This modules provides utility to patch instructions when moving them from one module to
//! another. For instance, functions or tables IDs needs to be remapped.

use walrus::ir;
use walrus::ir::{Instr, InstrSeqId, InstrSeqType};
use walrus::{
    FunctionBuilder, FunctionId, FunctionKind, GlobalId, InstrSeqBuilder, LocalFunction, MemoryId,
    Module, TableId,
};

use std::collections::{HashMap, HashSet};

use crate::Linker;

// ———————————————————————————— Function Cloning ———————————————————————————— //

/// Clones the given linkee function into the base module.
///
/// All necessary transformations (e.g. updating wasm items IDs) will be applied.
pub(crate) fn clone_func(
    linker: &mut Linker,
    base: &mut Module,
    linkee: &Module,
    func: &LocalFunction,
) -> FunctionId {
    let clonner = FunctionClonner::new(linker, base, linkee, func);
    clonner.clonne()
}

pub struct FunctionClonner<'a> {
    linker: &'a mut Linker,
    base: &'a mut Module,
    linkee: &'a Module,
    func: &'a LocalFunction,
    seq_map: HashMap<InstrSeqId, InstrSeqId>,
}

impl<'a> FunctionClonner<'a> {
    fn new(
        linker: &'a mut Linker,
        base: &'a mut Module,
        linkee: &'a Module,
        func: &'a LocalFunction,
    ) -> Self {
        Self {
            linker,
            base,
            linkee,
            func,
            seq_map: HashMap::new(),
        }
    }

    fn clonne(mut self) -> FunctionId {
        let body_instr_seq = self.func.block(self.func.entry_block());

        // Prepare arguments
        let ty = self.linkee.types.get(self.func.ty());
        let mut builder = FunctionBuilder::new(&mut self.base.types, ty.params(), ty.results());
        let mut args = Vec::with_capacity(self.func.args.len());

        // Are there always exactly as many parameters as there are locals?
        assert_eq!(self.func.args.len(), ty.params().len());

        for (param, old_local_id) in ty.params().iter().zip(self.func.args.iter()) {
            let new_local_id = self.base.locals.add(*param);
            self.linker.remap_local(*old_local_id, new_local_id);
            args.push(new_local_id);
        }

        // Clone function
        self.clone_instr_seq(body_instr_seq.id(), &mut builder.func_body());
        builder.finish(args, &mut self.base.funcs)
    }

    fn clone_instr_seq(&mut self, seq_id: InstrSeqId, builder: &mut InstrSeqBuilder) {
        let instr_seq = self.func.block(seq_id);
        for (instr, _) in instr_seq.iter() {
            let instr = self.patch_instr(instr, builder);
            builder.instr(instr);
        }
    }

    fn patch_instr(&mut self, instr: &Instr, builder: &mut InstrSeqBuilder) -> Instr {
        match instr {
            Instr::Block(block) => Instr::Block(ir::Block {
                seq: self.get_or_build_instr_seq(block.seq, builder),
            }),
            Instr::Loop(loop_) => Instr::Loop(ir::Loop {
                seq: self.get_or_build_instr_seq(loop_.seq, builder),
            }),
            Instr::Call(call) => Instr::Call(ir::Call {
                func: self.linker.new_func_id(call.func),
            }),
            Instr::CallIndirect(call) => Instr::CallIndirect(ir::CallIndirect {
                ty: self.linker.new_type_id(call.ty),
                table: self.linker.new_table_id(call.table),
            }),
            Instr::LocalGet(get) => Instr::LocalGet(ir::LocalGet {
                local: self.linker.new_local_id(get.local),
            }),
            Instr::LocalSet(set) => Instr::LocalSet(ir::LocalSet {
                local: self.linker.new_local_id(set.local),
            }),
            Instr::LocalTee(tee) => Instr::LocalTee(ir::LocalTee {
                local: self.linker.new_local_id(tee.local),
            }),
            Instr::GlobalGet(get) => Instr::GlobalGet(ir::GlobalGet {
                global: self.linker.new_global_id(get.global),
            }),
            Instr::GlobalSet(set) => Instr::GlobalSet(ir::GlobalSet {
                global: self.linker.new_global_id(set.global),
            }),
            Instr::Const(_) => instr.clone(),
            Instr::Binop(_) => instr.clone(),
            Instr::Unop(_) => instr.clone(),
            Instr::Select(_) => instr.clone(),
            Instr::Unreachable(_) => instr.clone(),
            Instr::Br(br) => Instr::Br(ir::Br {
                block: self.get_or_build_instr_seq(br.block, builder),
            }),
            Instr::BrIf(br_if) => Instr::BrIf(ir::BrIf {
                block: self.get_or_build_instr_seq(br_if.block, builder),
            }),
            Instr::IfElse(if_else) => Instr::IfElse(ir::IfElse {
                consequent: self.get_or_build_instr_seq(if_else.consequent, builder),
                alternative: self.get_or_build_instr_seq(if_else.alternative, builder),
            }),
            Instr::BrTable(br_table) => Instr::BrTable(ir::BrTable {
                blocks: br_table
                    .blocks
                    .iter()
                    .map(|seq_id| self.get_or_build_instr_seq(*seq_id, builder))
                    .collect(),
                default: self.get_or_build_instr_seq(br_table.default, builder),
            }),
            Instr::Drop(_) => instr.clone(),
            Instr::Return(_) => instr.clone(),
            Instr::MemorySize(mem) => Instr::MemorySize(ir::MemorySize {
                memory: self.linker.new_mem_id(mem.memory),
            }),
            Instr::MemoryGrow(mem) => Instr::MemoryGrow(ir::MemoryGrow {
                memory: self.linker.new_mem_id(mem.memory),
            }),
            Instr::MemoryInit(mem) => Instr::MemoryInit(ir::MemoryInit {
                memory: self.linker.new_mem_id(mem.memory),
                data: self.linker.new_data_id(mem.data),
            }),
            Instr::DataDrop(data) => Instr::DataDrop(ir::DataDrop {
                data: self.linker.new_data_id(data.data),
            }),
            Instr::MemoryCopy(mem) => Instr::MemoryCopy(ir::MemoryCopy {
                src: self.linker.new_mem_id(mem.src),
                dst: self.linker.new_mem_id(mem.dst),
            }),
            Instr::MemoryFill(mem) => Instr::MemoryFill(ir::MemoryFill {
                memory: self.linker.new_mem_id(mem.memory),
            }),
            Instr::Load(load) => Instr::Load(ir::Load {
                memory: self.linker.new_mem_id(load.memory),
                kind: load.kind,
                arg: load.arg,
            }),
            Instr::Store(store) => Instr::Store(ir::Store {
                memory: self.linker.new_mem_id(store.memory),
                kind: store.kind,
                arg: store.arg,
            }),
            Instr::AtomicRmw(atom) => Instr::AtomicRmw(ir::AtomicRmw {
                memory: self.linker.new_mem_id(atom.memory),
                op: atom.op,
                width: atom.width,
                arg: atom.arg,
            }),
            Instr::Cmpxchg(cmpxchg) => Instr::Cmpxchg(ir::Cmpxchg {
                memory: self.linker.new_mem_id(cmpxchg.memory),
                width: cmpxchg.width,
                arg: cmpxchg.arg,
            }),
            Instr::AtomicNotify(notify) => Instr::AtomicNotify(ir::AtomicNotify {
                memory: self.linker.new_mem_id(notify.memory),
                arg: notify.arg,
            }),
            Instr::AtomicWait(wait) => Instr::AtomicWait(ir::AtomicWait {
                memory: self.linker.new_mem_id(wait.memory),
                arg: wait.arg,
                sixty_four: wait.sixty_four,
            }),
            Instr::AtomicFence(_) => instr.clone(),
            Instr::TableGet(get) => Instr::TableGet(ir::TableGet {
                table: self.linker.new_table_id(get.table),
            }),
            Instr::TableSet(set) => Instr::TableSet(ir::TableSet {
                table: self.linker.new_table_id(set.table),
            }),
            Instr::TableGrow(grow) => Instr::TableGrow(ir::TableGrow {
                table: self.linker.new_table_id(grow.table),
            }),
            Instr::TableSize(size) => Instr::TableSize(ir::TableSize {
                table: self.linker.new_table_id(size.table),
            }),
            Instr::TableFill(fill) => Instr::TableFill(ir::TableFill {
                table: self.linker.new_table_id(fill.table),
            }),
            Instr::RefNull(_) => instr.clone(),
            Instr::RefIsNull(_) => instr.clone(),
            Instr::RefFunc(ref_func) => Instr::RefFunc(ir::RefFunc {
                func: self.linker.new_func_id(ref_func.func),
            }),
            Instr::V128Bitselect(_) => instr.clone(),
            Instr::I8x16Swizzle(_) => instr.clone(),
            Instr::I8x16Shuffle(_) => instr.clone(),
            Instr::LoadSimd(_) => instr.clone(),
            Instr::TableInit(init) => Instr::TableInit(ir::TableInit {
                table: self.linker.new_table_id(init.table),
                elem: self.linker.new_element_id(init.elem),
            }),
            Instr::ElemDrop(elem) => Instr::ElemDrop(ir::ElemDrop {
                elem: self.linker.new_element_id(elem.elem),
            }),
            Instr::TableCopy(copy) => Instr::TableCopy(ir::TableCopy {
                src: self.linker.new_table_id(copy.src),
                dst: self.linker.new_table_id(copy.dst),
            }),
        }
    }

    fn get_or_build_instr_seq(
        &mut self,
        seq_id: InstrSeqId,
        builder: &mut InstrSeqBuilder,
    ) -> InstrSeqId {
        // Return cached ID if already cloned
        if let Some(new_id) = self.seq_map.get(&seq_id) {
            return *new_id;
        }

        // Retrieve the sequence to clone and it's type
        let instr_seq = self.func.block(seq_id);
        let seq_type = match instr_seq.ty {
            InstrSeqType::Simple(results) => match results {
                Some(result) => InstrSeqType::new(&mut self.base.types, &[], &[result]),
                None => InstrSeqType::new(&mut self.base.types, &[], &[]),
            },
            InstrSeqType::MultiValue(type_id) => {
                let ty = self.linkee.types.get(type_id);
                InstrSeqType::new(&mut self.base.types, ty.params(), ty.results())
            }
        };

        // Build the new sequence in the base module
        let mut seq_builder = builder.dangling_instr_seq(seq_type);
        let new_seq_id = seq_builder.id();
        self.seq_map.insert(seq_id, new_seq_id);
        self.clone_instr_seq(seq_id, &mut seq_builder);

        new_seq_id
    }
}

// ————————————————————————————————— Patch —————————————————————————————————— //

/// A patch can be applied to replace some IDs by other IDs. For instance, an imported function can
/// be replaced by a concrete function that was added to the module during linking.
#[derive(Default)]
pub struct Patch {
    funcs: HashMap<FunctionId, FunctionId>,
    globs: HashMap<GlobalId, GlobalId>,
    tables: HashMap<TableId, TableId>,
    memories: HashMap<MemoryId, MemoryId>,
}

impl Patch {
    pub fn new() -> Self {
        Self::default()
    }

    fn patched_func_id(&self, id: FunctionId) -> FunctionId {
        match self.funcs.get(&id) {
            Some(id) => *id,
            None => id,
        }
    }

    fn patched_glob_id(&self, id: GlobalId) -> GlobalId {
        match self.globs.get(&id) {
            Some(id) => *id,
            None => id,
        }
    }

    fn patched_table_id(&self, id: TableId) -> TableId {
        match self.tables.get(&id) {
            Some(id) => *id,
            None => id,
        }
    }

    fn patched_memory_id(&self, id: MemoryId) -> MemoryId {
        match self.memories.get(&id) {
            Some(id) => *id,
            None => id,
        }
    }

    pub fn remap_func(&mut self, old: FunctionId, new: FunctionId) {
        self.funcs.insert(old, new);
    }

    pub fn remap_table(&mut self, old: TableId, new: TableId) {
        self.tables.insert(old, new);
    }

    pub fn remap_glob(&mut self, old: GlobalId, new: GlobalId) {
        self.globs.insert(old, new);
    }

    pub fn remap_memory(&mut self, old: MemoryId, new: MemoryId) {
        self.memories.insert(old, new);
    }

    pub fn patch(&self, module: &mut Module) {
        self.patch_funcs(module);
    }

    fn patch_funcs(&self, module: &mut Module) {
        for func in module.funcs.iter_mut() {
            match &mut func.kind {
                FunctionKind::Import(_) => {} // Ignore imported functions
                FunctionKind::Local(func) => self.patch_func(func),
                FunctionKind::Uninitialized(_) => panic!("Encountered uninitialized function"),
            }
        }
    }

    fn patch_func(&self, func: &mut LocalFunction) {
        let mut visited = HashSet::new();
        let mut to_patch = HashSet::new();
        self.patch_instr_seq(func.entry_block(), func, &mut visited, &mut to_patch);

        loop {
            if let Some(seq_id) = pop(&mut to_patch) {
                self.patch_instr_seq(seq_id, func, &mut visited, &mut to_patch);
            } else {
                break;
            }
        }
    }

    fn patch_instr_seq(
        &self,
        instr_seq_id: InstrSeqId,
        func: &mut LocalFunction,
        visited: &mut HashSet<InstrSeqId>,
        to_patch: &mut HashSet<InstrSeqId>,
    ) {
        if visited.insert(instr_seq_id) == false {
            // Already patched
            return;
        }

        for (instr, _) in func.block_mut(instr_seq_id).iter_mut() {
            self.patch_instr(instr, to_patch)
        }
    }

    fn patch_instr(&self, instr: &mut Instr, to_patch: &mut HashSet<InstrSeqId>) {
        match instr {
            Instr::Block(block) => {
                to_patch.insert(block.seq);
            }
            Instr::Loop(loop_) => {
                to_patch.insert(loop_.seq);
            }
            Instr::Call(call) => call.func = self.patched_func_id(call.func),
            Instr::CallIndirect(call) => call.table = self.patched_table_id(call.table),
            Instr::LocalGet(_) => {}
            Instr::LocalSet(_) => {}
            Instr::LocalTee(_) => {}
            Instr::GlobalGet(get) => get.global = self.patched_glob_id(get.global),
            Instr::GlobalSet(set) => set.global = self.patched_glob_id(set.global),
            Instr::Const(_) => {}
            Instr::Binop(_) => {}
            Instr::Unop(_) => {}
            Instr::Select(_) => {}
            Instr::Unreachable(_) => {}
            Instr::Br(br) => {
                to_patch.insert(br.block);
            }
            Instr::BrIf(br_if) => {
                to_patch.insert(br_if.block);
            }
            Instr::IfElse(if_else) => {
                to_patch.insert(if_else.consequent);
                to_patch.insert(if_else.alternative);
            }
            Instr::BrTable(br_table) => to_patch.extend(br_table.blocks.iter()),
            Instr::Drop(_) => {}
            Instr::Return(_) => {}
            Instr::MemorySize(mem) => mem.memory = self.patched_memory_id(mem.memory),
            Instr::MemoryGrow(mem) => mem.memory = self.patched_memory_id(mem.memory),
            Instr::MemoryInit(mem) => mem.memory = self.patched_memory_id(mem.memory),
            Instr::DataDrop(_) => {}
            Instr::MemoryCopy(mem) => {
                mem.src = self.patched_memory_id(mem.src);
                mem.dst = self.patched_memory_id(mem.dst);
            }
            Instr::MemoryFill(mem) => mem.memory = self.patched_memory_id(mem.memory),
            Instr::Load(load) => load.memory = self.patched_memory_id(load.memory),
            Instr::Store(store) => store.memory = self.patched_memory_id(store.memory),
            Instr::AtomicRmw(atom) => atom.memory = self.patched_memory_id(atom.memory),
            Instr::Cmpxchg(cmpxchg) => cmpxchg.memory = self.patched_memory_id(cmpxchg.memory),
            Instr::AtomicNotify(notify) => notify.memory = self.patched_memory_id(notify.memory),
            Instr::AtomicWait(wait) => wait.memory = self.patched_memory_id(wait.memory),
            Instr::AtomicFence(_) => {}
            Instr::TableGet(get) => get.table = self.patched_table_id(get.table),
            Instr::TableSet(set) => set.table = self.patched_table_id(set.table),
            Instr::TableGrow(grow) => grow.table = self.patched_table_id(grow.table),
            Instr::TableSize(size) => size.table = self.patched_table_id(size.table),
            Instr::TableFill(fill) => fill.table = self.patched_table_id(fill.table),
            Instr::RefNull(_) => {}
            Instr::RefIsNull(_) => {}
            Instr::RefFunc(ref_func) => ref_func.func = self.patched_func_id(ref_func.func),
            Instr::V128Bitselect(_) => {}
            Instr::I8x16Swizzle(_) => {}
            Instr::I8x16Shuffle(_) => {}
            Instr::LoadSimd(_) => {}
            Instr::TableInit(init) => init.table = self.patched_table_id(init.table),
            Instr::ElemDrop(_) => {}
            Instr::TableCopy(copy) => {
                copy.src = self.patched_table_id(copy.src);
                copy.dst = self.patched_table_id(copy.dst);
            }
        }
    }
}

/// Pops an item from a set, if not empty.
fn pop<T>(set: &mut HashSet<T>) -> Option<T>
where
    T: Copy + Eq + std::hash::Hash,
{
    match set.iter().next() {
        Some(item) => {
            let item = *item;
            set.remove(&item);
            Some(item)
        }
        None => None,
    }
}
