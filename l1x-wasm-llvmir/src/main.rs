use clap::Parser;
use std::path::PathBuf;
use l1x_wasm_llvmir::translate_module_to_file_by_path;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    wasm_path: PathBuf,
    #[arg(short, long)]
    output: Option<PathBuf>,
}

// Main function init
fn main() {
    env_logger::init();

    let Cli { wasm_path, output } = Cli::parse();

    let output = output.unwrap_or_else(|| wasm_path.with_extension("ll"));
    match translate_module_to_file_by_path(&wasm_path, &output) {
        Ok(_) => {
            println!("Written to {:?}", output);
        }
        Err(e) => {
            panic!("FAILED: {}", e);
        }
    }
}
