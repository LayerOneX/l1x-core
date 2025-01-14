extern crate l1x_consensus_primitives;

type Gas = l1x_consensus_primitives::Gas;

#[derive(Debug, Clone, Copy)]
pub struct Fee {
	pub basic_instruction: Gas,
	pub api_call: Gas,
}

impl Fee {
	pub fn create_basic_config() -> Self {
		Self { basic_instruction: 1, api_call: 1 }
	}
}
