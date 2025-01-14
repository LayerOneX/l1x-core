use primitives::*;

#[derive(Debug)]
pub struct VmFunctionCallOutcome {
	pub result: EventData,
	pub burnt_gas: Gas,
}
