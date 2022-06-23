(module
    (import "coral" "print_char" (func $print_char (type $print_char_t)))
    (import "coral" "buffer_write" (func $buffer_write (type $buffer_write_t)))
    (import "coral" "handles" (table $handles 2 2 externref))
    (type $print_char_t (func (param i32)))
    (type $buffer_write_t (func (param externref) (param i64) (param i64) (param i64)))
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

        ;; Call buffer_write
        i32.const 0
        table.get $handles
        i64.const 0
        i64.const 0
        i64.const 1
        call $buffer_write

        ;; Return value
        i32.const 42
    )
    (export "init" (func $init))
)
