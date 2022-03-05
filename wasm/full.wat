(module
  (import "mod" "add"
    (func $h (type $t_h))
  )
  (type $t_h (func (param $x i32) (param $y i32) (result i32)))
  (func $f (param $lhs i32) (param $rhs i32) (result i32)
    i32.const 0;; Target memory address

    get_local $lhs
    get_local $rhs
    call $h
    call $g

    ;; Store and load the result to addr 0
    i32.store
    i32.const 0
    i32.load)
  (func $g (param $x i32) (result i32)
    get_local $x
    i32.const 2
    i32.mul)
  (export "double_add" (func $f))
  (memory $mem 1)
)
