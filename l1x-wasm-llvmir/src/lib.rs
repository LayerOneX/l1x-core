pub mod environment_impl;

use anyhow::anyhow;
use std::path::PathBuf;

use cranelift_wasm::WasmResult;
use environment_impl::EnvironmentImpl;
use inkwell::{context::Context, memory_buffer::MemoryBuffer};

/// Translates the specified WASM data to LLVM IR and returns [`MemoryBuffer`]
pub fn translate_module_to_membuf(wasm_data: &Vec<u8>) -> Result<MemoryBuffer, anyhow::Error> {
    let llvm_context = Context::create();
    let mut environ = EnvironmentImpl::new(&llvm_context);

    match cranelift_wasm::translate_module(&wasm_data, &mut environ) {
        WasmResult::Ok(_) => Ok(environ.write_bitcode_to_memory()), //environ.print_to_stderr(),
        WasmResult::Err(e) => Err(anyhow!("Can't translate module: {}", e)),
    }
}

/// Translates the specified WASM data to LLVM IR and writes the LLVN IR data by the specified output path
pub fn translate_module_to_file(
    wasm_data: &Vec<u8>,
    output: &PathBuf,
) -> Result<(), anyhow::Error> {
    let llvm_context = Context::create();
    let mut environ = EnvironmentImpl::new(&llvm_context);

    match cranelift_wasm::translate_module(&wasm_data, &mut environ) {
        WasmResult::Ok(_) => {
            if let Err(e) = environ.print_to_file(output) {
                Err(anyhow!(
                    "Can't write to '{}': {}",
                    output.to_string_lossy(),
                    e
                ))
            } else {
                Ok(())
            }
        }
        WasmResult::Err(e) => Err(anyhow!("Can't translate module: {}", e.to_string())),
    }
}

/// Reads the WASM file and translates it to LLVM IR. LLVM IR will be written data by the specified output path
pub fn translate_module_to_file_by_path(
    wasm_path: &PathBuf,
    output: &PathBuf,
) -> Result<(), anyhow::Error> {
    let wasm_data = wat::parse_file(wasm_path.clone()).map_err(|e| {
        anyhow!(
            "Can't parse '{}'. Error: {}",
            wasm_path.to_string_lossy(),
            e
        )
    })?;

    translate_module_to_file(&wasm_data, output)?;

    Ok(())
}
