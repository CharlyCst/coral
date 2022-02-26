use std::collections::HashMap;

use crate::traits::{Module, Allocator};

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
    pub fn instance(module: impl Module, alloc: &impl Allocator) -> Self {

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

    // fn instantiate<Alloc>(&self, alloc: &mut Alloc) -> traits::ModuleResult<Self::Instance>
    // where
    //     Alloc: traits::Allocator,
    // {
    //     let code_size = self.code.len();
    //     let code_ptr = alloc.alloc_code(code_size as u32);

    //     // SAFETY: We rely on the correctness of the allocator that must return a pointer to an
    //     // unused memory region of the appropriate size.
    //     unsafe {
    //         let code = core::slice::from_raw_parts_mut(code_ptr, code_size);
    //         code.copy_from_slice(&self.code);
    //         self.apply_relocs(code, code_ptr as i64)?;
    //     };

    //     let mut instance = Instance::new();

    //     // Collect exported symbols
    //     let info = &self.info;
    //     for (exported_name, name) in &info.exported_names {
    //         let item = &info.items[&name];
    //         let symbol = match item {
    //             ModuleItem::Func(idx) => {
    //                 let func = &info.funs[*idx];
    //                 let addr = code_ptr.wrapping_add(func.offset as usize);
    //                 Symbol::Function { addr }
    //             }
    //             ModuleItem::Heap(_) => todo!("Handle memory export"),
    //         };
    //         instance.symbols.insert(exported_name.to_owned(), symbol);
    //     }

    //     // Instantiate data structures (e.g. heaps, tables...)
    //     let (vmctx, heaps) = self.build_datastructures(alloc);
    //     instance.vmctx = vmctx;
    //     instance.heaps = heaps;

    //     Ok(instance)
    // }

}

