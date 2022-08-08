use core::cmp::Ord;
use walrus::ir;
use walrus::ir::InstrSeqId;
use walrus::{self, LocalFunction};
use walrus::{
    FunctionBuilder, FunctionId, FunctionKind, ImportKind, Module, ModuleFunctions, ModuleLocals,
    ModuleTypes, TableId, ValType,
};

use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
pub struct Table(TableId);

/// An object representing a transformation to apply.
pub enum Patch {
    /// Transformations on an imported functions.
    ImportedFunc(ImportedFuncPatch),
}

#[derive(Default)]
pub struct ImportedFuncPatch {
    args: Vec<(u32, Table)>,
}

impl ImportedFuncPatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn replace_with_handle(&mut self, args_index: u32, table: Table) {
        self.args.push((args_index, table));
    }

    pub fn as_patch(mut self) -> Patch {
        self.args.sort_by(|x, y| x.0.cmp(&y.0));
        Patch::ImportedFunc(self)
    }
}

pub struct Patcher {
    module: Module,
    patch_set: HashMap<String, HashMap<String, Patch>>,
}

impl Patcher {
    pub fn new(wasm: &[u8]) -> walrus::Result<Self> {
        let config = walrus::ModuleConfig::new();
        let module = config.parse(wasm)?;

        Ok(Self {
            module,
            patch_set: HashMap::new(),
        })
    }

    /// Registers a patch to apply to modules.
    pub fn register_patch(&mut self, module: &str, name: &str, patch: Patch) {
        let module_patches = match self.patch_set.get_mut(module) {
            Some(patches) => patches,
            None => {
                self.patch_set.insert(String::from(module), HashMap::new());
                self.patch_set.get_mut(module).unwrap()
            }
        };
        module_patches.insert(String::from(name), patch);
    }

    pub fn add_table_import(
        &mut self,
        module: &str,
        name: &str,
        initial_capacity: u32,
        max_capacity: Option<u32>,
    ) -> Table {
        let (table_id, _) = self.module.add_import_table(
            module,
            name,
            initial_capacity,
            max_capacity,
            ValType::Externref,
        );

        Table(table_id)
    }

    pub fn add_table(&mut self, initial_capacity: u32, max_capacity: Option<u32>) -> Table {
        let table_id =
            self.module
                .tables
                .add_local(initial_capacity, max_capacity, ValType::Externref);
        Table(table_id)
    }

    pub fn patch(mut self) -> walrus::Result<Module> {
        self.patch_funcs();

        Ok(self.module)
    }

    fn patch_funcs(&mut self) {
        let mut func_patch = HashMap::new();
        let mut ignore = HashSet::new();
        let module = &mut self.module;

        for import in module.imports.iter() {
            if let Some(module_patch) = self.patch_set.get(&import.module) {
                if let Some(Patch::ImportedFunc(patch)) = module_patch.get(&import.name) {
                    if let ImportKind::Function(func_id) = import.kind {
                        let patched_func_id = create_proxy_func(
                            &mut module.funcs,
                            &mut module.types,
                            &mut module.locals,
                            func_id,
                            patch,
                        );
                        func_patch.insert(func_id, patched_func_id);
                        ignore.insert(patched_func_id);
                    }
                }
            }
        }

        replace_funcs_in_module(module, &func_patch, &ignore);
    }
}

fn create_proxy_func(
    funcs: &mut ModuleFunctions,
    types: &mut ModuleTypes,
    locals: &mut ModuleLocals,
    func_id: FunctionId,
    patch: &ImportedFuncPatch,
) -> FunctionId {
    let func = funcs.get_mut(func_id);
    let func_type = types.get(func.ty()).clone();

    // Update func type
    let patched_type = types.add(
        &patch_params(func_type.params(), patch),
        func_type.results(),
    );
    match &mut func.kind {
        walrus::FunctionKind::Import(imported) => imported.ty = patched_type,
        _ => todo!("Only imported functions are supported"),
    }

    let mut builder = FunctionBuilder::new(types, func_type.params(), func_type.results());
    let mut body = builder.func_body();
    let mut params = Vec::new();
    let mut patch_iter = patch.args.iter();
    let mut patch = patch_iter.next();
    for (idx, param) in func_type.params().iter().enumerate() {
        let param = locals.add(*param);
        params.push(param);
        body.local_get(param);

        if let Some((patch_idx, table)) = patch {
            if idx == *patch_idx as usize {
                body.table_get(table.0);
            }
            patch = patch_iter.next();
        }
    }
    body.call(func_id);
    builder.finish(params, funcs)
}

fn patch_params(params: &[ValType], patch: &ImportedFuncPatch) -> Vec<ValType> {
    let mut patched_params = Vec::with_capacity(params.len());

    let mut patch_iter = patch.args.iter();
    let mut patch = patch_iter.next();
    for (idx, param) in params.iter().enumerate() {
        let ty = match patch {
            Some((patch_idx, _)) if *patch_idx as usize == idx => {
                patch = patch_iter.next();
                ValType::Externref
            }
            _ => *param,
        };
        patched_params.push(ty);
    }

    patched_params
}

fn replace_funcs_in_module(
    module: &mut Module,
    func_patch: &HashMap<FunctionId, FunctionId>,
    ignore: &HashSet<FunctionId>,
) {
    for func in module.funcs.iter_mut() {
        if ignore.contains(&func.id()) {
            continue;
        }
        if let FunctionKind::Local(ref mut func) = func.kind {
            replace_funcs_in_body(func, func_patch);
        }
    }
}

fn replace_funcs_in_body(func: &mut LocalFunction, func_patch: &HashMap<FunctionId, FunctionId>) {
    let entry_block = func.entry_block();
    let mut visited_blocks = HashSet::new();
    replace_funcs_in_block(func, entry_block, &mut visited_blocks, func_patch);
}

fn replace_funcs_in_block(
    func: &mut LocalFunction,
    block_id: InstrSeqId,
    visited_blocks: &mut HashSet<InstrSeqId>,
    func_patch: &HashMap<FunctionId, FunctionId>,
) {
    let block = func.block_mut(block_id);
    let mut next_blocks = Vec::new();
    for (instr, _) in block.iter_mut() {
        match instr {
            ir::Instr::Call(call) => {
                if let Some(patched_func_id) = func_patch.get(&call.func) {
                    *instr = ir::Instr::Call(ir::Call {
                        func: *patched_func_id,
                    });
                }
            }
            ir::Instr::Block(block) => next_blocks.push(block.seq),
            ir::Instr::Loop(block) => next_blocks.push(block.seq),
            ir::Instr::BrIf(block) => next_blocks.push(block.block),
            ir::Instr::IfElse(block) => {
                next_blocks.push(block.consequent);
                next_blocks.push(block.alternative);
            }
            ir::Instr::BrTable(table) => {
                for block in table.blocks.iter() {
                    next_blocks.push(*block);
                }
            }
            _ => (),
        }
    }

    for block in next_blocks {
        if !visited_blocks.contains(&block) {
            visited_blocks.insert(block);
            replace_funcs_in_block(func, block, visited_blocks, func_patch);
        }
    }
}
