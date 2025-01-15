(module
  (func (param i32 i32) (result i32)
    (local i32)
    local.get 0
    i32.const 3
    loop (param i32 i32) (result i32)  ;; label = @1
      i32.sub
      local.tee 2
      local.get 0
      i32.const 0
      i32.ne
      if  ;; label = @2
        local.get 2
        i32.const 4
        i32.const 0
        local.set 0
        br 1 (;@1;)
      end
    end
    return)
  (export "myFunc" (func 0)))