//! WebAssembmy Types

use alloc::vec::Vec;

/// A WebAssembly function type.
#[derive(Clone, Debug)]
pub struct FuncType {
    args: Vec<ValueType>,
    ret: Vec<ValueType>,
}

impl FuncType {
    pub fn new(args: Vec<ValueType>, ret: Vec<ValueType>) -> Self {
        Self { args, ret }
    }

    pub fn eq(&self, other: &Self) -> bool {
        self.args == other.args && self.ret == other.ret
    }

    pub fn args(&self) -> &[ValueType] {
        &self.args
    }

    pub fn ret(&self) -> &[ValueType] {
        &self.ret
    }
}

/// A WebAssembly value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    I32,
    I64,
    F32,
    F64,
    ExternRef,
    FuncRef,
}

/// A WebAssembly numeric type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumType {
    I32,
    I64,
    F32,
    F64,
}

/// A WebAssembly reference type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefType {
    ExternRef,
    FuncRef,
}
