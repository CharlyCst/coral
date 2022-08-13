(module
  ;; Types
  (type $vma_write
    (func
      (param $source externref)
      (param $target externref)
      (param $source_offset i64)
      (param $target_offset i64)
      (param $size i64)))
  (type $pub_vma_write
    (func
      (param $source i32)
      (param $target i32)
      (param $source_offset i64)
      (param $target_offset i64)
      (param $size i64)))

  ;; Imports
  (import "coral" "vma_write"
    (func $vma_write
      (type $vma_write)))
  (import "coral" "handles"
    (table $handles 2 4 externref))

  ;; Definitions
  (table $module 4 externref)
  (func $pub_vma_write
    (export "vma_write")
    (type $pub_vma_write)
      local.get 0
      table.get $module
      local.get 1
      table.get $handles
      local.get 2
      local.get 3
      local.get 4
      call $vma_write)
)
