#[derive(thiserror::Error, Debug)]
#[allow(missing_docs)]
pub enum EbpfVmError {
	#[error("VM Execution Mbuff Error: buffer too small ({0:?}), cannot use data_offset {1:?} and data_end_offset {2:?}")]
	VMExecMBuffTooSmall(usize, usize, usize),

	#[error("VM Execution Error: No program set, call prog_set() to load one")]
	VMExecNoProgramSet,

	#[error("VM Execution Memory Out of Bounds Error: {0}")]
	VMExecMemoryOutOfBound(#[from] std::io::Error),

	#[error("VM Execution CALL Error:  Max call depth exceeded, instruction ptr={0}, instruction offset={1}")]
	VMExecCallDepthMax(usize, usize),

	#[error("VM Execution Ctx Error:  unknown helper function ID {0}")]
	VMExecCtxInvalidHelperFunctionId(u64),

	#[error("VM Execution Ctx Call Error: {0}")]
	VMExecCtxCallError(String),

	#[error("VM out of gas: gas_limit={0}")]
	VMOutOfGas(u64),
}
