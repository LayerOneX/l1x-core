(module
    (func (export "myFunc") (param i32) (param i32) (result i32)
    local.get 0
    local.get 1
    i32.sub
    i32.const 1
    (block (param i32) (param i32) (result i32)
        i32.sub ;; param1 - param2 - 1
    )
    return
    )
)