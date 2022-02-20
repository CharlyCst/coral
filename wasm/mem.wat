(module
  (func $double_add (param $lhs i32) (param $rhs i32) (result i32)
    i32.const 0;; Target memory address

    get_local $lhs
    get_local $rhs
    i32.add
    call $double

    ;; Store and load the result to addr 0
    i32.store
    i32.const 0
    i32.load)
  (func $double (param $x i32) (result i32)
    get_local $x
    i32.const 2
    i32.mul)
  (export "double_add" (func $double_add))
  (memory $mem 1)
)
