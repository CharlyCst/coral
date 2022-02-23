(module
  (func $double_add (param $lhs i32) (param $rhs i32) (result i32)
    i32.const 0
    i32.load)
  (export "double_add" (func $double_add))
  (memory $mem 1)
)
