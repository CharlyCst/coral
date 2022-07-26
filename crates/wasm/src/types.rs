//! WebAssembmy Types

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumType {
    I32,
    I64,
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefType {
    ExternRef,
    FuncRef,
}
