use inkwell::{context::Context, memory_buffer::MemoryBuffer, values::GenericValue};
use std::{fs, path::Path};
use l1x_wasm_llvmir::translate_module_to_membuf;
use wasmtime::{self, ValType};

#[derive(Debug, PartialEq, Clone)]
enum ParamType {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl ParamType {
    fn create_generic_value<'a>(&self, llvm_context: &'a Context) -> GenericValue<'a> {
        match self {
            Self::I64(v) => llvm_context
                .i64_type()
                .create_generic_value(*v as u64, true),
            Self::I32(v) => llvm_context
                .i32_type()
                .create_generic_value(*v as u64, true),
            Self::F64(v) => llvm_context.f64_type().create_generic_value(*v),
            Self::F32(v) => llvm_context.f32_type().create_generic_value(f64::from(*v)),
        }
    }
}

impl Into<wasmtime::Val> for &ParamType {
    fn into(self) -> wasmtime::Val {
        match self {
            ParamType::I64(v) => wasmtime::Val::I64(*v),
            ParamType::I32(v) => wasmtime::Val::I32(*v),
            ParamType::F64(v) => wasmtime::Val::F64(v.to_bits()),
            ParamType::F32(v) => wasmtime::Val::F32(v.to_bits()),
        }
    }
}

impl Into<wasmtime::Val> for ParamType {
    fn into(self) -> wasmtime::Val {
        (&self).into()
    }
}

impl From<&wasmtime::Val> for ParamType {
    fn from(other: &wasmtime::Val) -> Self {
        match other {
            wasmtime::Val::I64(v) => Self::I64(*v),
            wasmtime::Val::I32(v) => Self::I32(*v),
            wasmtime::Val::F64(v) => Self::F64(f64::from_bits(*v)),
            wasmtime::Val::F32(v) => Self::F32(f32::from_bits(*v)),
            _ => unimplemented!(),
        }
    }
}

impl From<wasmtime::Val> for ParamType {
    fn from(other: wasmtime::Val) -> Self {
        Self::from(&other)
    }
}

fn read_wat_module(path: &Path) -> Vec<u8> {
    match path.extension() {
        None => {
            panic!("the file extension is not wasm or wat");
        }
        Some(ext) => match ext.to_str() {
            Some("wasm") => std::fs::read(path).expect("error reading wasm file"),
            Some("wat") => wat::parse_file(path)
                .map_err(|e| e.to_string())
                .expect("failed to parse wat"),
            None | Some(&_) => panic!("the file extension for {:?} is not wasm or wat", path),
        },
    }
}

fn parse_wat(wat: &str) -> Vec<u8> {
    wat::parse_str(wat)
        .map_err(|e| e.to_string())
        .expect("failed to parse wat from str")
}

fn verify_translated_code(
    llvmir_bitcode_buffer: MemoryBuffer,
    wat_data: &Vec<u8>,
    params: &Vec<ParamType>,
    fn_name: &str,
    return_: Option<ParamType>,
) {
    let llvm_context = Context::create();
    let llvm_module = match llvm_context.create_module_from_ir(llvmir_bitcode_buffer) {
        Ok(module) => module,
        Err(e) => {
            panic!("{}", e);
        }
    };

    let llvmir_res = match llvm_module.create_interpreter_execution_engine() {
        Ok(engine) => {
            let test_fn = llvm_module.get_function(fn_name).expect(&format!(
                "Can't find \"{fn_name}\" function in LLVM IR. Translated LLVM code:\n\n{}",
                llvm_module.print_to_string().to_str().unwrap()
            ));
            let args: Vec<GenericValue> = params
                .iter()
                .map(|e| e.create_generic_value(&llvm_context))
                .collect();
            let args_refs = args.iter().map(|v| v).collect::<Vec<&GenericValue>>();
            let res = unsafe { engine.run_function(test_fn, &args_refs) };

            res.as_int(true)
        }
        Err(e) => panic!("{}", e),
    };

    let wasm_engine = wasmtime::Engine::default();
    let wasm_module =
        wasmtime::Module::new(&wasm_engine, wat_data).expect("Can't parse wat module");
    let mut wasm_store = wasmtime::Store::new(&wasm_engine, 4);
    let wasm_instance = wasmtime::Instance::new(&mut wasm_store, &wasm_module, &[])
        .expect("Can't create wasm instance");

    let test_fn = wasm_instance
        .get_func(&mut wasm_store, fn_name)
        .expect("Can't find test function");

    let result_type = test_fn.ty(&wasm_store).results().collect::<Vec<ValType>>();
    if result_type.len() != 1 {
        unimplemented!()
    }

    let mut results = if let Some(ref return_) = return_ {
        [return_.into()]
    } else {
        match result_type[0] {
            ValType::I32 => [wasmtime::Val::I32(0)],
            ValType::I64 => [wasmtime::Val::I64(0)],
            _ => unimplemented!(),
        }
    };

    let args = params
        .iter()
        .map(|v| v.into())
        .collect::<Vec<wasmtime::Val>>();
    let wat_res = match test_fn.call(&mut wasm_store, &args, &mut results) {
        Ok(_) => results
            .get(0)
            .expect("Wat fucntion has not returned any results"),
        Err(e) => panic!("{}", e),
    };

    if let Some(ref return_) = return_ {
        assert_eq!(
            &ParamType::from(wat_res),
            return_,
            "The actual WASM return values isn't equal to the defined one"
        );
    }
    match result_type[0] {
        ValType::I32 => {
            assert_eq!(
                wat_res.i32().expect("Wat function returned not i32 type"),
                llvmir_res as i32,
                "A WASM return value not equal to a LLVM IR return value. Translated LLVM code:\n\n{}",
                llvm_module.print_to_string().to_str().unwrap()
            );
        }
        ValType::I64 => {
            assert_eq!(
                wat_res.i64().expect("Wat function returned not i32 type"),
                llvmir_res as i64,
                "A WASM return value not equal to a LLVM IR return value. Translated LLVM code:\n\n{}",
                llvm_module.print_to_string().to_str().unwrap()
            );
        }
        _ => unimplemented!(),
    }
}

#[test]
fn testsuite() {
    let mut paths: Vec<_> = fs::read_dir("./wat")
        .unwrap()
        .map(|r| r.unwrap())
        .filter(|p| {
            // Ignore files starting with `.`, which could be editor temporary files
            if let Some(stem) = p.path().file_stem() {
                if let Some(stemstr) = stem.to_str() {
                    return !stemstr.starts_with('.');
                }
            }
            false
        })
        .collect();
    paths.sort_by_key(|dir| dir.path());
    for path in paths {
        let path = path.path();
        println!("=== {} ===", path.display());
        let wat_data = read_wat_module(&path);
        let llvmir_bitcode_buffer = translate_module_to_membuf(&wat_data).unwrap();

        verify_translated_code(
            llvmir_bitcode_buffer,
            &wat_data,
            &vec![ParamType::I32(2), ParamType::I32(10)],
            "myFunc",
            None,
        );
    }
}

fn run_test(wat: &str, params: &Vec<ParamType>, return_: Option<ParamType>) {
    let wat_data = parse_wat(wat);
    let llvmir_bitcode_buffer = translate_module_to_membuf(&wat_data).unwrap();

    verify_translated_code(llvmir_bitcode_buffer, &wat_data, params, "myFunc", return_);
}

fn run_test_fn(wat: &str, fn_name: &str, params: &Vec<ParamType>, return_: ParamType) {
    let wat_data = parse_wat(wat);
    let llvmir_bitcode_buffer = translate_module_to_membuf(&wat_data).unwrap();

    verify_translated_code(
        llvmir_bitcode_buffer,
        &wat_data,
        params,
        fn_name,
        Some(return_),
    );
}

#[test]
fn block_operation() {
    let wat = r#"(module
        (func (export "myFunc") (param i64) (result i32)
        (block (result i32 i64 i32)
            i32.const 8
            i64.const 7
            i32.const 9
        )
        drop
        drop
        )
    )"#;
    run_test(wat, &vec![ParamType::I64(0)], Some(ParamType::I32(8)));
}

#[test]
fn i32wrap_i64_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i64) (result i32)
            local.get 0
            i32.wrap_i64
            )
        )"#;

    run_test(
        wat,
        &vec![ParamType::I64(0x1f_ffff_ffff)],
        Some(ParamType::I32(-1)),
    );
    run_test(
        wat,
        &vec![ParamType::I64(0xffff_ffff)],
        Some(ParamType::I32(-1)),
    );
}

#[test]
fn i64extend_i32_operation() {
    let wat_s = r#"(module
            (func (export "myFunc") (param i32) (result i64)
            local.get 0
            i64.extend_i32_s
            )
        )"#;

    run_test(wat_s, &vec![ParamType::I32(-10)], None);
    run_test(wat_s, &vec![ParamType::I32(10)], None);

    let wat_u = r#"(module
            (func (export "myFunc") (param i32) (result i64)
            local.get 0
            i64.extend_i32_u
            )
        )"#;

    run_test(wat_u, &vec![ParamType::I32(-10)], None);
    run_test(wat_u, &vec![ParamType::I32(10)], None);
}

#[ignore]
#[test]
fn clz_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            i32.clz
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
    run_test(wat, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat, &vec![ParamType::I32(0), ParamType::I32(0)], None);
}

#[ignore]
#[test]
fn ctz_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            i32.ctz
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
    run_test(wat, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat, &vec![ParamType::I32(0), ParamType::I32(0)], None);
}

#[test]
fn popcnt_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            i32.popcnt
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
}

#[test]
fn add_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.add
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(2), ParamType::I32(10)], None);
}

#[test]
fn sub_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.sub
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(2), ParamType::I32(10)], None);
}

#[test]
fn mul_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.mul
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(2), ParamType::I32(10)], None);
}

#[test]
fn div_operation() {
    let wat_div_u = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.div_u
            )
        )"#;

    run_test(
        wat_div_u,
        &vec![ParamType::I32(10), ParamType::I32(2)],
        None,
    );

    // let wat_div_s = r#"(module
    //     (func (export "myFunc") (param i32) (param i32) (result i32)
    //     local.get 0
    //     local.get 1
    //     i32.div_s
    //     )
    // )"#;

    // run_test(
    //     wat_div_s,
    //     &vec![ParamType::I32(-10), ParamType::I32(2)],
    //     None,
    // );
}

#[test]
fn rem_operation() {
    let wat_u = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.rem_u
            )
        )"#;

    run_test(wat_u, &vec![ParamType::I32(10), ParamType::I32(2)], None);
}

#[test]
fn and_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.and
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
}

#[test]
fn or_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.or
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
}

#[test]
fn xor_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.xor
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
}

#[test]
fn shl_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.shl
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
    run_test(
        wat,
        &vec![ParamType::I32(1), ParamType::I32(32)],
        Some(ParamType::I32(1)),
    );
}

#[test]
fn shr_operation() {
    let wat_u = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.shr_u
            )
        )"#;

    run_test(wat_u, &vec![ParamType::I32(-10), ParamType::I32(2)], None);

    let wat_s = r#"(module
        (func (export "myFunc") (param i32) (param i32) (result i32)
        local.get 0
        local.get 1
        i32.shr_s
        )
    )"#;

    run_test(wat_s, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
}

#[test]
fn rotl_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.rotl
            )
        )"#;

    run_test(
        wat,
        &vec![ParamType::I32(1073741825), ParamType::I32(2)],
        None,
    );
}

#[test]
fn rotr_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.rotr
            )
        )"#;

    run_test(
        wat,
        &vec![ParamType::I32(1073741825), ParamType::I32(2)],
        None,
    );
}

#[test]
fn eqz_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            i32.eqz
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
    run_test(wat, &vec![ParamType::I32(0), ParamType::I32(2)], None);
}

#[test]
fn eq_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.eq
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(10)], None);
}

#[test]
fn ne_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.ne
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(10)], None);
}

#[test]
fn lt_operation() {
    let wat_u = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.lt_u
            )
        )"#;

    run_test(wat_u, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat_u, &vec![ParamType::I32(2), ParamType::I32(-10)], None);

    let wat_s = r#"(module
        (func (export "myFunc") (param i32) (param i32) (result i32)
        local.get 0
        local.get 1
        i32.lt_s
        )
    )"#;

    run_test(wat_s, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat_s, &vec![ParamType::I32(2), ParamType::I32(-10)], None);
}

#[test]
fn gt_operation() {
    let wat_u = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.gt_u
            )
        )"#;

    run_test(wat_u, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat_u, &vec![ParamType::I32(2), ParamType::I32(-10)], None);

    let wat_s = r#"(module
        (func (export "myFunc") (param i32) (param i32) (result i32)
        local.get 0
        local.get 1
        i32.gt_s
        )
    )"#;

    run_test(wat_s, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat_s, &vec![ParamType::I32(2), ParamType::I32(-10)], None);
}

#[test]
fn le_operation() {
    let wat_u = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.le_u
            )
        )"#;

    run_test(wat_u, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat_u, &vec![ParamType::I32(2), ParamType::I32(-10)], None);
    run_test(wat_u, &vec![ParamType::I32(10), ParamType::I32(-10)], None);

    let wat_s = r#"(module
        (func (export "myFunc") (param i32) (param i32) (result i32)
        local.get 0
        local.get 1
        i32.le_s
        )
    )"#;

    run_test(wat_s, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat_s, &vec![ParamType::I32(2), ParamType::I32(-10)], None);
    run_test(wat_s, &vec![ParamType::I32(10), ParamType::I32(-10)], None);
}

#[test]
fn ge_operation() {
    let wat_u = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.ge_u
            )
        )"#;

    run_test(wat_u, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat_u, &vec![ParamType::I32(2), ParamType::I32(-10)], None);
    run_test(wat_u, &vec![ParamType::I32(10), ParamType::I32(-10)], None);

    let wat_s = r#"(module
        (func (export "myFunc") (param i32) (param i32) (result i32)
        local.get 0
        local.get 1
        i32.ge_s
        )
    )"#;

    run_test(wat_s, &vec![ParamType::I32(-10), ParamType::I32(2)], None);
    run_test(wat_s, &vec![ParamType::I32(2), ParamType::I32(-10)], None);
    run_test(wat_s, &vec![ParamType::I32(10), ParamType::I32(-10)], None);
}

#[test]
fn drop_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            drop
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10), ParamType::I32(2)], None);
}

#[test]
fn select_operation() {
    let wat = r#"(module
            (func (export "myFunc") (param i32) (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            local.get 2
            select
            )
        )"#;

    run_test(
        wat,
        &vec![ParamType::I32(10), ParamType::I32(2), ParamType::I32(3)],
        None,
    );
    run_test(
        wat,
        &vec![ParamType::I32(10), ParamType::I32(2), ParamType::I32(0)],
        None,
    );
}

#[test]
fn global_operation() {
    let wat = r#"(module
            (global i32 (i32.const 42))
            (func (export "myFunc") (param i32) (result i32)
            global.get 0
            )
        )"#;

    run_test(wat, &vec![ParamType::I32(10)], None);

    let wat = r#"(module
        (global i32 (i32.const 42))
        (func (export "myFunc") (param i32) (result i32)
        global.get 0
        local.get 0
        i32.eq
        )
    )"#;

    run_test(wat, &vec![ParamType::I32(10)], None);

    let wat = r#"(module
        (global (mut i32) (i32.const 42))
        (func (export "myFunc") (param i32) (result i32)
        local.get 0
        global.set 0
        global.get 0
        )
    )"#;

    run_test(wat, &vec![ParamType::I32(10)], None);
}

#[test]
fn call_operation() {
    let wat = r#"(module
        (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.const 3
            call $sub
        )
        (func $sub (param i32) (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.sub
        )
    )"#;
    run_test(
        wat,
        &vec![ParamType::I32(2), ParamType::I32(10)],
        Some(ParamType::I32(-8)),
    );
}

#[test]
fn call_with_6_params_operation() {
    let wat = r#"(module
        (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.const 2
            i32.const 3
            i32.const 4
            i32.const 5 ;; six arguments
            call $sub
        )
        (func $sub (param i32) (param i32) (param i32) (param i32) (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            i32.sub
        )
    )"#;
    run_test(
        wat,
        &vec![ParamType::I32(2), ParamType::I32(10)],
        Some(ParamType::I32(-8)),
    );
}

#[test]
fn unreachable_operation() {
    let wat = r#"(module
        (func (export "myFunc") (param i32) (param i32) (result i32)
            local.get 0
            local.get 1
            br 0
            unreachable
        )
    )"#;
    run_test(
        wat,
        &vec![ParamType::I32(2), ParamType::I32(10)],
        Some(ParamType::I32(10)),
    );
}

#[ignore = "Require eBPF runtime"]
#[test]
fn memory_load_operation() {
    let get_wat = |load_instr| {
        format!(
            r#"(module
                    (memory 1)
                    (data (i32.const 0) "001111222\ff")
                    (func (export "myFunc") (param i32) (result i32)
                        i32.const 2
                        {}
                    )
                )"#,
            load_instr
        )
    };
    run_test(
        get_wat("i32.load").as_str(),
        &vec![ParamType::I32(10)],
        Some(ParamType::I32(0x31313131)),
    );
    run_test(
        get_wat("i32.load16_u").as_str(),
        &vec![ParamType::I32(10)],
        Some(ParamType::I32(0x3131)),
    );
    run_test(
        get_wat("i32.load8_u").as_str(),
        &vec![ParamType::I32(10)],
        Some(ParamType::I32(0x31)),
    );
    run_test(
        get_wat("i32.load offset=4").as_str(),
        &vec![ParamType::I32(10)],
        Some(ParamType::I32(-13487566)),
    );
    run_test(
        get_wat("i32.load16_s offset=6").as_str(),
        &vec![ParamType::I32(10)],
        Some(ParamType::I32(-206)),
    );
    run_test(
        get_wat("i32.load8_s offset=7").as_str(),
        &vec![ParamType::I32(10)],
        Some(ParamType::I32(-1)),
    );
}

#[ignore = "Require eBPF runtime"]
#[test]
fn memory_load_store_offset_operation() {
    let wat = r#"(module
        (memory 1)
        (data (i32.const 1) "001111")
        (func (export "myFunc") (param i32) (result i32)
            i32.const 2
            local.get 0
            i32.store offset=4
            i32.const 2
            i32.load offset=3
        )
    )"#;
    run_test(
        wat,
        &vec![ParamType::I32(0xffff3232u32 as i32)],
        Some(ParamType::I32(-13487567)),
    );
}

#[ignore = "Require eBPF runtime"]
#[test]
fn memory_store_operation() {
    let get_wat = |store_instr| {
        format!(
            r#"(module
                    (memory 1)
                    (func (export "myFunc") (param i32) (result i32)
                        i32.const 2
                        local.get 0
                        {}
                        i32.const 2
                        i32.load
                    )
                )"#,
            store_instr
        )
    };
    run_test(
        get_wat("i32.store").as_str(),
        &vec![ParamType::I32(0xffffff32u32 as i32)],
        Some(ParamType::I32(-206)),
    );
    run_test(
        get_wat("i32.store16").as_str(),
        &vec![ParamType::I32(0xffffff32u32 as i32)],
        Some(ParamType::I32(65330)),
    );
    run_test(
        get_wat("i32.store8").as_str(),
        &vec![ParamType::I32(0xffffff32u32 as i32)],
        Some(ParamType::I32(50)),
    );
}

#[ignore = "Require eBPF runtime"]
#[test]
fn memory_test() {
    let wat = r#"(module
        (memory 1)
        (data (i32.const 0) "ABC\a7D") (data (i32.const 20) "WASM")

        ;; Data section
        (func (export "data") (result i32)
          (i32.and
            (i32.and
              (i32.and
                (i32.eq (i32.load8_u (i32.const 0)) (i32.const 65))
                (i32.eq (i32.load8_u (i32.const 3)) (i32.const 167))
              )
              (i32.and
                (i32.eq (i32.load8_u (i32.const 6)) (i32.const 0))
                (i32.eq (i32.load8_u (i32.const 19)) (i32.const 0))
              )
            )
            (i32.and
              (i32.and
                (i32.eq (i32.load8_u (i32.const 20)) (i32.const 87))
                (i32.eq (i32.load8_u (i32.const 23)) (i32.const 77))
              )
              (i32.and
                (i32.eq (i32.load8_u (i32.const 24)) (i32.const 0))
                (i32.eq (i32.load8_u (i32.const 1023)) (i32.const 0))
              )
            )
          )
        )

        ;; Memory cast
        (func (export "cast") (result f64)
          (i64.store (i32.const 8) (i64.const -12345))
          (if
            (f64.eq
              (f64.load (i32.const 8))
              (f64.reinterpret_i64 (i64.const -12345))
            )
            (then (return (f64.const 0)))
          )
          (i64.store align=1 (i32.const 9) (i64.const 0))
          (i32.store16 align=1 (i32.const 15) (i32.const 16453))
          (f64.load align=1 (i32.const 9))
        )

        ;; Sign and zero extending memory loads
        (func (export "i32_load8_s") (param $i i32) (result i32)
          (i32.store8 (i32.const 8) (local.get $i))
          (i32.load8_s (i32.const 8))
        )
        (func (export "i32_load8_u") (param $i i32) (result i32)
          (i32.store8 (i32.const 8) (local.get $i))
          (i32.load8_u (i32.const 8))
        )
        (func (export "i32_load16_s") (param $i i32) (result i32)
          (i32.store16 (i32.const 8) (local.get $i))
          (i32.load16_s (i32.const 8))
        )
        (func (export "i32_load16_u") (param $i i32) (result i32)
          (i32.store16 (i32.const 8) (local.get $i))
          (i32.load16_u (i32.const 8))
        )
        (func (export "i64_load8_s") (param $i i64) (result i64)
          (i64.store8 (i32.const 8) (local.get $i))
          (i64.load8_s (i32.const 8))
        )
        (func (export "i64_load8_u") (param $i i64) (result i64)
          (i64.store8 (i32.const 8) (local.get $i))
          (i64.load8_u (i32.const 8))
        )
        (func (export "i64_load16_s") (param $i i64) (result i64)
          (i64.store16 (i32.const 8) (local.get $i))
          (i64.load16_s (i32.const 8))
        )
        (func (export "i64_load16_u") (param $i i64) (result i64)
          (i64.store16 (i32.const 8) (local.get $i))
          (i64.load16_u (i32.const 8))
        )
        (func (export "i64_load32_s") (param $i i64) (result i64)
          (i64.store32 (i32.const 8) (local.get $i))
          (i64.load32_s (i32.const 8))
        )
        (func (export "i64_load32_u") (param $i i64) (result i64)
          (i64.store32 (i32.const 8) (local.get $i))
          (i64.load32_u (i32.const 8))
        )
      )"#;

    run_test_fn(wat, "data", &vec![], ParamType::I32(1));
    run_test_fn(
        wat,
        "i32_load8_s",
        &vec![ParamType::I32(-1)],
        ParamType::I32(-1),
    );
    run_test_fn(
        wat,
        "i32_load8_u",
        &vec![ParamType::I32(-1)],
        ParamType::I32(255),
    );
    run_test_fn(
        wat,
        "i32_load16_s",
        &vec![ParamType::I32(-1)],
        ParamType::I32(-1),
    );
    run_test_fn(
        wat,
        "i32_load16_u",
        &vec![ParamType::I32(-1)],
        ParamType::I32(65535),
    );
    run_test_fn(
        wat,
        "i32_load8_s",
        &vec![ParamType::I32(100)],
        ParamType::I32(100),
    );
    run_test_fn(
        wat,
        "i32_load8_u",
        &vec![ParamType::I32(200)],
        ParamType::I32(200),
    );
    run_test_fn(
        wat,
        "i32_load16_s",
        &vec![ParamType::I32(20000)],
        ParamType::I32(20000),
    );
    run_test_fn(
        wat,
        "i32_load16_u",
        &vec![ParamType::I32(40000)],
        ParamType::I32(40000),
    );
    run_test_fn(
        wat,
        "i32_load8_s",
        &vec![ParamType::I32(0xfedc6543u32 as i32)],
        ParamType::I32(0x43),
    );
    run_test_fn(
        wat,
        "i32_load8_s",
        &vec![ParamType::I32(0x3456cdef)],
        ParamType::I32(0xffffffefu32 as i32),
    );
    run_test_fn(
        wat,
        "i32_load8_u",
        &vec![ParamType::I32(0xfedc6543u32 as i32)],
        ParamType::I32(0x43),
    );
    run_test_fn(
        wat,
        "i32_load8_u",
        &vec![ParamType::I32(0x3456cdef)],
        ParamType::I32(0xef),
    );
    run_test_fn(
        wat,
        "i32_load16_s",
        &vec![ParamType::I32(0xfedc6543u32 as i32)],
        ParamType::I32(0x6543),
    );
    run_test_fn(
        wat,
        "i32_load16_s",
        &vec![ParamType::I32(0x3456cdef)],
        ParamType::I32(0xffffcdefu32 as i32),
    );
    run_test_fn(
        wat,
        "i32_load16_u",
        &vec![ParamType::I32(0xfedc6543u32 as i32)],
        ParamType::I32(0x6543),
    );
    run_test_fn(
        wat,
        "i32_load16_u",
        &vec![ParamType::I32(0x3456cdef)],
        ParamType::I32(0xcdef),
    );
    run_test_fn(
        wat,
        "i64_load8_s",
        &vec![ParamType::I64(-1)],
        ParamType::I64(-1),
    );
    run_test_fn(
        wat,
        "i64_load8_u",
        &vec![ParamType::I64(-1)],
        ParamType::I64(255),
    );
    run_test_fn(
        wat,
        "i64_load16_s",
        &vec![ParamType::I64(-1)],
        ParamType::I64(-1),
    );
    run_test_fn(
        wat,
        "i64_load16_u",
        &vec![ParamType::I64(-1)],
        ParamType::I64(65535),
    );
    run_test_fn(
        wat,
        "i64_load32_s",
        &vec![ParamType::I64(-1)],
        ParamType::I64(-1),
    );
    run_test_fn(
        wat,
        "i64_load32_u",
        &vec![ParamType::I64(-1)],
        ParamType::I64(4294967295),
    );
    run_test_fn(
        wat,
        "i64_load8_s",
        &vec![ParamType::I64(100)],
        ParamType::I64(100),
    );
    run_test_fn(
        wat,
        "i64_load8_u",
        &vec![ParamType::I64(200)],
        ParamType::I64(200),
    );
    run_test_fn(
        wat,
        "i64_load16_s",
        &vec![ParamType::I64(20000)],
        ParamType::I64(20000),
    );
    run_test_fn(
        wat,
        "i64_load16_u",
        &vec![ParamType::I64(40000)],
        ParamType::I64(40000),
    );
    run_test_fn(
        wat,
        "i64_load32_s",
        &vec![ParamType::I64(20000)],
        ParamType::I64(20000),
    );
    run_test_fn(
        wat,
        "i64_load32_u",
        &vec![ParamType::I64(40000)],
        ParamType::I64(40000),
    );
    run_test_fn(
        wat,
        "i64_load8_s",
        &vec![ParamType::I64(0xfedcba9856346543u64 as i64)],
        ParamType::I64(0x43),
    );
    run_test_fn(
        wat,
        "i64_load8_s",
        &vec![ParamType::I64(0x3456436598bacdef)],
        ParamType::I64(0xffffffffffffffefu64 as i64),
    );
    run_test_fn(
        wat,
        "i64_load8_u",
        &vec![ParamType::I64(0xfedcba9856346543u64 as i64)],
        ParamType::I64(0x43),
    );
    run_test_fn(
        wat,
        "i64_load8_u",
        &vec![ParamType::I64(0x3456436598bacdef)],
        ParamType::I64(0xef),
    );
    run_test_fn(
        wat,
        "i64_load16_s",
        &vec![ParamType::I64(0xfedcba9856346543u64 as i64)],
        ParamType::I64(0x6543),
    );
    run_test_fn(
        wat,
        "i64_load16_s",
        &vec![ParamType::I64(0x3456436598bacdef)],
        ParamType::I64(0xffffffffffffcdefu64 as i64),
    );
    run_test_fn(
        wat,
        "i64_load16_u",
        &vec![ParamType::I64(0xfedcba9856346543u64 as i64)],
        ParamType::I64(0x6543),
    );
    run_test_fn(
        wat,
        "i64_load16_u",
        &vec![ParamType::I64(0x3456436598bacdef)],
        ParamType::I64(0xcdef),
    );
    run_test_fn(
        wat,
        "i64_load32_s",
        &vec![ParamType::I64(0xfedcba9856346543u64 as i64)],
        ParamType::I64(0x56346543),
    );
    run_test_fn(
        wat,
        "i64_load32_s",
        &vec![ParamType::I64(0x3456436598bacdef)],
        ParamType::I64(0xffffffff98bacdefu64 as i64),
    );
    run_test_fn(
        wat,
        "i64_load32_u",
        &vec![ParamType::I64(0xfedcba9856346543u64 as i64)],
        ParamType::I64(0x56346543),
    );
    run_test_fn(
        wat,
        "i64_load32_u",
        &vec![ParamType::I64(0x3456436598bacdef)],
        ParamType::I64(0x98bacdef),
    );
}
