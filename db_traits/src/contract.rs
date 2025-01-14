use anyhow::Error;
use async_trait::async_trait;
use primitives::*;

use system::contract::Contract;

#[async_trait]
pub trait ContractState {
	async fn get_all_contract(&self) -> Result<Vec<Contract>, Error>;

	async fn get_contract(&self, address: &Address) -> Result<Contract, Error>;

	async fn get_contract_owner(&self, address: &Address) -> Result<(AccessType, Address), Error>;

	async fn is_valid_contract(&self, address: &Address) -> Result<bool, Error>;
}
