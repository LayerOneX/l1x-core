use alloc::vec::Vec;
use evm_interpreter::ExitResult;
use primitive_types::H160;

#[derive(Clone, Debug, Copy)]
pub enum MergeStrategy {
	/// Fully commit the sub-layer into the parent. This happens if the sub-machine executes
	/// successfully.
	Commit,
	/// Revert the state, but keep remaining gases. This happens with the `REVERT` opcode.
	Revert,
	/// Discard the state and gases. This happens in all situations where the machine encounters an
	/// error.
	Discard,
}

pub trait PrecompileSet<S, H> {
	/// Attempt to execute the precompile at the given `code_address`. Returns
	/// `None` if it's not a precompile.
	fn execute(
		&self,
		code_address: H160,
		input: &[u8],
		state: &mut S,
		handler: &mut H,
	) -> Option<(ExitResult, Vec<u8>)>;
}

impl<S, H> PrecompileSet<S, H> for () {
	fn execute(
		&self,
		_code_address: H160,
		_input: &[u8],
		_state: &mut S,
		_handler: &mut H,
	) -> Option<(ExitResult, Vec<u8>)> {
		None
	}
}
