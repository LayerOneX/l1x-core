use anyhow::Error;
use async_trait::async_trait;
use primitives::*;

use system::contract_instance::ContractInstance;

#[async_trait]
pub trait ContractInstanceState {
	async fn get_contract_instance(
		&self,
		instance_address: &Address,
	) -> Result<ContractInstance, Error>;

	async fn store_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
		value: &ContractInstanceValue,
	) -> Result<(), Error>;

	async fn update_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
		value: &ContractInstanceValue,
	) -> Result<(), Error>;

	async fn delete_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
	) -> Result<(), Error>;

	async fn get_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error>;

	async fn is_valid_contract_instance(&self, instance_address: &Address) -> Result<bool, Error>;
}
