(module
    (import "coral" "print_char" (func $print_char (type $t)))
    (type $t (func (param i32)))
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

        ;; Return value
        i32.const 42
    )
    (export "init" (func $init))
)
