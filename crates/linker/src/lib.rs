mod instr;

use std::collections::{HashMap, HashSet};
use walrus::{
    DataId, ElementId, ExportItem, FunctionId, FunctionKind, GlobalId, GlobalKind, ImportId,
    ImportKind, LocalId, MemoryId, Module, TableId, TypeId,
};

/// Links a base module with another provided module.
pub fn link(base: &mut Module, linkee: &Module, linkee_name: &str) {
    Linker::new(linkee_name.to_string()).link(base, linkee)
}

pub(crate) struct Linker {
    globals_map: HashMap<GlobalId, GlobalId>,
    tables_map: HashMap<TableId, TableId>,
    funcs_map: HashMap<FunctionId, FunctionId>,
    types_map: HashMap<TypeId, TypeId>,
    locals_map: HashMap<LocalId, LocalId>,
    memories_map: HashMap<MemoryId, MemoryId>,
    data_map: HashMap<DataId, DataId>,
    elements_map: HashMap<ElementId, ElementId>,
    linkee_imports: HashSet<ImportId>,
    linkee_name: String,
}

impl Linker {
    fn new(linkee_name: String) -> Self {
        Self {
            globals_map: HashMap::new(),
            tables_map: HashMap::new(),
            funcs_map: HashMap::new(),
            types_map: HashMap::new(),
            locals_map: HashMap::new(),
            memories_map: HashMap::new(),
            data_map: HashMap::new(),
            elements_map: HashMap::new(),
            linkee_imports: HashSet::new(),
            linkee_name,
        }
    }

    pub(crate) fn new_func_id(&self, id: FunctionId) -> FunctionId {
        self.funcs_map[&id]
    }

    pub(crate) fn new_global_id(&self, id: GlobalId) -> GlobalId {
        self.globals_map[&id]
    }

    pub(crate) fn new_table_id(&self, id: TableId) -> TableId {
        self.tables_map[&id]
    }

    pub(crate) fn new_type_id(&self, id: TypeId) -> TypeId {
        self.types_map[&id]
    }

    pub(crate) fn new_local_id(&self, id: LocalId) -> LocalId {
        self.locals_map[&id]
    }

    pub(crate) fn new_mem_id(&self, id: MemoryId) -> MemoryId {
        self.memories_map[&id]
    }

    pub(crate) fn new_data_id(&self, id: DataId) -> DataId {
        self.data_map[&id]
    }

    pub(crate) fn new_element_id(&self, id: ElementId) -> ElementId {
        self.elements_map[&id]
    }

    pub(crate) fn remap_local(&mut self, old: LocalId, new: LocalId) {
        self.locals_map.insert(old, new);
    }

    fn link(mut self, base: &mut Module, linkee: &Module) {
        self.merge_tables(base, linkee);
        self.merge_globals(base, linkee);
        self.merge_data(base, linkee);
        self.merge_elements(base, linkee);
        self.merge_funcs(base, linkee);
        self.remove_resolved_imports(base, linkee);
    }

    fn merge_tables(&mut self, base: &mut Module, linkee: &Module) {
        for table in linkee.tables.iter() {
            let new_id = if let Some(import_id) = table.import {
                let import = linkee.imports.get(import_id);
                let (table_id, import_id) = base.add_import_table(
                    &import.module,
                    &import.name,
                    table.initial,
                    table.maximum,
                    table.element_ty,
                );
                self.linkee_imports.insert(import_id);
                table_id
            } else {
                base.tables
                    .add_local(table.initial, table.maximum, table.element_ty)
            };
            self.tables_map.insert(table.id(), new_id);
        }
    }

    fn merge_globals(&mut self, base: &mut Module, linkee: &Module) {
        for global in linkee.globals.iter() {
            let new_id = match global.kind {
                GlobalKind::Import(import_id) => {
                    let import = linkee.imports.get(import_id);
                    let (glob_id, import_id) = base.add_import_global(
                        &import.module,
                        &import.name,
                        global.ty,
                        global.mutable,
                    );
                    self.linkee_imports.insert(import_id);
                    glob_id
                }
                GlobalKind::Local(init_expr) => {
                    base.globals.add_local(global.ty, global.mutable, init_expr)
                }
            };
            self.globals_map.insert(global.id(), new_id);
        }
    }

    fn merge_data(&mut self, _base: &mut Module, linkee: &Module) {
        for _segment in linkee.data.iter() {
            todo!("Linking modules with data segments is not yet supported");
        }
    }

    fn merge_elements(&mut self, _base: &mut Module, linkee: &Module) {
        for _element in linkee.elements.iter() {
            todo!("Linking modules with element segments is not yet supported");
        }
    }

    fn merge_funcs(&mut self, base: &mut Module, linkee: &Module) {
        for func in linkee.funcs.iter() {
            let func_id = match func.kind {
                FunctionKind::Import(ref func) => {
                    let import_id = func.import;
                    let import = linkee.imports.get(import_id);
                    let ty = linkee.types.get(func.ty);
                    let ty_id = base.types.add(ty.params(), ty.results());
                    let (func_id, import_id) =
                        base.add_import_func(&import.module, &import.name, ty_id);
                    self.linkee_imports.insert(import_id);
                    func_id
                }
                FunctionKind::Local(ref func) => instr::clone_func(self, base, linkee, func),
                FunctionKind::Uninitialized(_) => panic!("Encountered uninitialized function"),
            };
            self.funcs_map.insert(func.id(), func_id);
        }
    }

    fn remove_resolved_imports(&mut self, base: &mut Module, linkee: &Module) {
        let mut to_delete = Vec::new();
        let mut patch = instr::Patch::new();
        for import in base.imports.iter() {
            if self.linkee_imports.contains(&import.id()) {
                // This import comes from the linkee, so don't try to resolve it
                continue;
            }
            if import.module != self.linkee_name {
                continue;
            }

            // TODO: better error messages
            let export = linkee
                .exports
                .iter()
                .find(|export| export.name == import.name)
                .expect(&format!(
                    "Missing export: {}.{}",
                    import.module, import.name
                ));

            match import.kind {
                ImportKind::Function(func_id) => {
                    let linkee_func_id = match export.item {
                        ExportItem::Function(func_id) => func_id,
                        _ => panic!(
                            "Invalid export type: {}.{}, expected a function",
                            import.module, import.name
                        ),
                    };
                    let new_func_id = self.new_func_id(linkee_func_id);
                    patch.remap_func(func_id, new_func_id);
                }
                ImportKind::Table(table_id) => {
                    let linkee_table_id = match export.item {
                        ExportItem::Table(table_id) => table_id,
                        _ => panic!(
                            "Invalid export type: {}.{}, expected a table",
                            import.module, import.name
                        ),
                    };
                    let new_table_id = self.new_table_id(linkee_table_id);
                    patch.remap_table(table_id, new_table_id);
                }
                ImportKind::Memory(mem_id) => {
                    let linkee_mem_id = match export.item {
                        ExportItem::Memory(mem_id) => mem_id,
                        _ => panic!(
                            "Invalid export type: {}.{}, expected a memory",
                            import.module, import.name
                        ),
                    };
                    let new_mem_id = self.new_mem_id(linkee_mem_id);
                    patch.remap_memory(mem_id, new_mem_id);
                }
                ImportKind::Global(glob_id) => {
                    let linkee_glob_id = match export.item {
                        ExportItem::Global(glob_id) => glob_id,
                        _ => panic!(
                            "Invalid export type: {}.{}, expected a global",
                            import.module, import.name
                        ),
                    };
                    let new_glob_id = self.new_global_id(linkee_glob_id);
                    patch.remap_glob(glob_id, new_glob_id);
                }
            }

            to_delete.push(import.id());
        }

        patch.patch(base);
        for import_id in to_delete {
            base.imports.delete(import_id);
        }
    }
}
