(module
   (type $_type (func (param i32) (param i32) (result i32))) ;; we need a signature for the indirect call

   (table 2 funcref) ;; Table with function pointers
   (elem (i32.const 0) $sub $sub1) ;; function pointer with index 0 points to $sub function

   (func $sub (param $p1 i32) (param $p2 i32) (result i32)
          local.get $p1
          local.get $p2
          i32.sub
   )
    (func $sub1 (param $p1 i32) (param $p2 i32) (result i32)
        local.get $p1
        i32.const 1
        i32.sub
    )


   (func $sub2 (param $p1 i32) (param $p2 i32) (result i32)
	       local.get $p1
	       local.get $p2
           i32.const 0
	  (call_indirect (type $_type)) ;; we need the signature of the function for validation
   )

    (export "sub" (func $sub))
    (export "myFunc" (func $sub2))
)