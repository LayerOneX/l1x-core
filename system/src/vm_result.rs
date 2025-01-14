use primitives::*;

#[derive(Debug)]
pub struct VmResult {
	pub result: VMExecutionResult,
	pub contract_instance_address: Address,
	pub burnt_gas: Gas,
}
