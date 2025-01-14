use primitives::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ContractInstance {
	pub instance_address: Address,
	pub contract_address: Address,
	pub owner_address: Address,
}

impl ContractInstance {
	pub fn new(
		instance_address: Address,
		contract_address: Address,
		owner_address: Address,
	) -> ContractInstance {
		ContractInstance { instance_address, contract_address, owner_address }
	}
}
