use crate::contract_state::ContractState;
use anyhow::Error;
use primitives::*;
use system::contract::Contract;

pub struct ContractManager<'a> {
	pub contract: Contract,
	pub contract_state: ContractState<'a>,
}

impl<'a> ContractManager<'a> {
	pub async fn new(
		contract_address: Address,
		access: AccessType,
		r#type: ContractType,
		code: ContractCode,
		owner_address: Address,
		contract_state: ContractState<'a>,
	) -> Result<ContractManager, Error> {
		let contract = Contract::new(contract_address, access, r#type, code, owner_address);
		contract_state.store_contract(&contract).await?;
		let manager = ContractManager { contract, contract_state };
		Ok(manager)
	}

	pub fn get_code(&self) -> ContractCode {
		self.contract.code.clone()
	}
}
