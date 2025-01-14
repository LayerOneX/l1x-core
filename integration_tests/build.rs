use std::process::Command;

fn main() {
	println!("cargo:rerun-if-changed=fixtures/l1x-test-contract/src/lib.rs");

	let output = Command::new("cargo")
		.arg("l1x")
		.arg("build")
		.current_dir("fixtures/l1x-test-contract")
		.output()
		.expect("Failed to run cargo l1x build");

	assert!(output.status.success());
}
