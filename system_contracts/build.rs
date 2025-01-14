use std::io;

const CONTRACTS_DIRS: [&str; 2] = ["./contracts_binaries", "./contracts_init_data"];

fn main() -> io::Result<()> {
    CONTRACTS_DIRS.iter().for_each(|dir| {
        println!("cargo:rerun-if-changed={}", dir);
    });
    Ok(())
}
