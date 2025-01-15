(module
    (func (export "myFunc") (param i32) (param i32) (result i32)
    local.get 0
    local.get 0
    local.get 0
    i32.const 42
    i32.le_u
    if (param i32) (result i32)
        i32.const 1
        i32.add
    else
        i32.const 2
        i32.sub
    end

    i32.const 1
    i32.eq
    if (param i32) (result i32)
        i32.const 100
        i32.add
    end
    return
    )
)