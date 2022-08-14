//! Coral System Calls

type ExternRef = u32;

#[link(wasm_import_module = "coral")]
extern "C" {
    pub fn vma_write(
        source: ExternRef,
        target: ExternRef,
        source_offset: u64,
        target_offset: u64,
        size: u64,
    );

    pub fn module_create(
        source: ExternRef,
        offset: u64,
        size: u64,
    ) -> ExternRef;
}

