(module
  ;; Types
  (type $vma_write
    (func
      (param $source externref)
      (param $target externref)
      (param $source_offset i64)
      (param $target_offset i64)
      (param $size i64)
      (result i32)))
  (type $pub_vma_write
    (func
      (param $source i32)
      (param $target i32)
      (param $source_offset i64)
      (param $target_offset i64)
      (param $size i64)
      (result i32)))
  (type $module_create
    (func
      (param $source externref)
      (param $offset i64)
      (param $size   i64)
      (result i32)
      (result externref)))
  (type $pub_module_create
    (func
      (param $source i32)
      (param $offset i64)
      (param $size   i64)
      (result i32)
      (result i32)))
  (type $component_create
    (func (result i32 externref)))
  (type $pub_component_create
    (func (result i32 i32)))
  (type $component_add_instance
    (func
      (param $component externref)
      (param $module    externref)
      (result i32 i32)))
  (type $pub_component_add_instance
    (func
      (param $component i32)
      (param $module    i32)
      (result i32 i32)))

  ;; Imports
  (import "coral" "vma_write"
    (func $vma_write
      (type $vma_write)))
  (import "coral" "module_create"
    (func $module_create
      (type $module_create)))
  (import "coral" "component_create"
    (func $component_create
      (type $component_create)))
  (import "coral" "component_add_instance"
    (func $component_add_instance
      (type $component_add_instance)))
  (import "coral" "handles"
    (table $handles 2 4 externref))

  ;; Definitions
  (table $vma       4 externref)
  (table $module    4 externref)
  (table $component 4 externref)
  (global $nb_modules    (mut i32) (i32.const 0))
  (global $nb_components (mut i32) (i32.const 0))

  (func $pub_vma_write
    (export "vma_write")
    (type $pub_vma_write)
      local.get 0
      table.get $vma
      local.get 1
      table.get $handles
      local.get 2
      local.get 3
      local.get 4
      call $vma_write)

  (func $pub_module_create
    (export "module_create")
    (type $pub_module_create)
      ;; Prepare index in module table
      global.get $nb_modules ;; return value
      global.get $nb_modules ;; used by table.set

      ;; Increment number of modules
      global.get $nb_modules
      i32.const 1
      i32.add
      global.set $nb_modules

      ;; Prepare syscall arguments & execute syscall
      local.get 0
      table.get $vma
      local.get 1
      local.get 2
      call $module_create

      ;; Store the module handle
      table.set $module)

  (func $pub_component_create
    (export "component_create")
    (type $pub_component_create)
      ;; Prepare index in module table
      global.get $nb_components ;; return value
      global.get $nb_components ;; used by table.set

      ;; Increment number of components
      global.get $nb_modules
      i32.const 1
      i32.add
      global.set $nb_components

      ;; Execute syscall
      call $component_create

      ;; Store component handle
      table.set $component)

  (func $pub_component_add_instance
    (export "component_add_instance")
    (type $pub_component_add_instance)
      local.get 0
      table.get $component
      local.get 1
      table.get $module
      call $component_add_instance
    )
)
