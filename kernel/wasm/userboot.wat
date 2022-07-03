(module
    (import "coral" "print_char" (func $print_char (type $print_char_t)))
    (import "coral" "buffer_write" (func $buffer_write (type $buffer_write_t)))
    (import "coral" "handles" (table $handles 2 2 externref))
    (type $print_char_t (func (param i32)))
    (type $buffer_write_t (func (param externref) (param i64) (param i64) (param i64)))
    (memory $buffer 1 1)
    (func $init (result i32)
        ;; Print greeting message
        i32.const 0x48 ;; H
        call $print_char
        i32.const 0x65 ;; e
        call $print_char
        i32.const 0x6C ;; l
        call $print_char
        i32.const 0x6C ;; l
        call $print_char
        i32.const 0x6F ;; o
        call $print_char
        i32.const 0x20 ;;
        call $print_char
        i32.const 0x66 ;; f
        call $print_char
        i32.const 0x72 ;; r
        call $print_char
        i32.const 0x6F ;; o
        call $print_char
        i32.const 0x6D ;; m
        call $print_char
        i32.const 0x20 ;;
        call $print_char
        i32.const 0x75 ;; u
        call $print_char
        i32.const 0x73 ;; s
        call $print_char
        i32.const 0x65 ;; e
        call $print_char
        i32.const 0x72 ;; r
        call $print_char
        i32.const 0x73 ;; s
        call $print_char
        i32.const 0x70 ;; p
        call $print_char
        i32.const 0x61 ;; a
        call $print_char
        i32.const 0x63 ;; c
        call $print_char
        i32.const 0x65 ;; e
        call $print_char
        i32.const 0x21 ;; !
        call $print_char
        i32.const 0x0A ;; \n
        call $print_char

        ;; Write to buffer
        i32.const 2     ;; x
        i32.const 4     ;; y
        i32.const 0x61  ;; character
        i32.const 14    ;; color
        call $write_byte
        call $flush

        ;; Return value
        i32.const 42
    )
    (func $write_byte
        (param $x i32)
        (param $y i32)
        (param $char i32)
        (param $color i32)
        (local $index i32)

        ;; Compute buffer index: (80 * y + x) * 2
        i32.const 80
        local.get $y
        i32.mul
        local.get $x
        i32.add
        i32.const 2
        i32.mul
        local.set $index

        ;; Store character
        local.get $char
        local.get $index
        i32.store8

        ;; Store color
        local.get $color
        local.get $index
        i32.const 1
        i32.add
        i32.store8
    )
    (func $flush
        ;; Host buffer handle
        i32.const 0
        table.get $handles

        ;; Buffer offset in wasm memory
        i64.const 0

        ;; Host buffer offset
        i64.const 0

        ;; Buffer size
        i64.const 25
        i64.const 80
        i64.mul
        i64.const 2
        i64.mul

        ;; Syscall
        call $buffer_write
    )
    (export "init" (func $init))
)
