use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, contract_instance::ContractInstanceState};
use std::fs::remove_dir_all;

use primitives::{Address, ContractInstanceKey, ContractInstanceValue};
use rocksdb::DB;
use system::contract_instance::ContractInstance;
pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}
#[async_trait]
impl BaseState<ContractInstance> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, contract_instance: &ContractInstance) -> Result<(), Error> {
		let key = format!(
			"contract_instance_contract_code_map_instance_address:{:?}",
			&contract_instance.instance_address
		);
		let value = serde_json::to_string(&contract_instance)?;
		self.db.put(key, value)?;

		let key = format!(
			"contract_instance_contract_code_map_contract_address:{:?}",
			&contract_instance.contract_address
		);
		let value = serde_json::to_string(&contract_instance)?;
		self.db.put(key, value)?;

		let key = format!(
			"contract_instance_contract_code_map_owner_address:{:?}",
			&contract_instance.owner_address
		);
		let value = serde_json::to_string(&contract_instance)?;
		self.db.put(key, value)?;

		Ok(())
	}

	async fn update(&self, _contract_instance: &ContractInstance) -> Result<(), Error> {
		// Implementation for update method
		todo!()
	}

	async fn raw_query(&self, _query: &str) -> Result<(), Error> {
		// Remove the database directory
		remove_dir_all(&self.db_path)?;

		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}
}

#[async_trait]
impl ContractInstanceState for StateRock {
	async fn get_contract_instance(
		&self,
		instance_address: &Address,
	) -> Result<ContractInstance, Error> {
		let key =
			format!("contract_instance_contract_code_map_instance_address:{:?}", instance_address);

		let contract_instance: ContractInstance = match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				serde_json::from_str(&value_str).map_err(Error::new)
			},
			None => Err(Error::msg("contract instance not found")),
		}?;

		Ok(contract_instance)
	}

	async fn store_state_key_value(
		&self,
		instance_address: &Address,
		contract_instance_key: &ContractInstanceKey,
		contract_instance_value: &ContractInstanceValue,
	) -> Result<(), Error> {
		let key = format!(
			"contract_instance_instance_address:{:?}_contract_instance_key:{:?}",
			instance_address, contract_instance_key
		);
		let value = serde_json::to_string(&contract_instance_value)?;
		match self.db.put(key, value) {
			Ok(_) => Ok(()),
			Err(err) => Err(anyhow!("Error inserting contract instance key-value - {}", err)),
		}
	}

	async fn update_state_key_value(
		&self,
		instance_address: &Address,
		contract_instance_key: &ContractInstanceKey,
		contract_instance_value: &ContractInstanceValue,
	) -> Result<(), Error> {
		let key = format!(
			"contract_instance_instance_address:{:?}_contract_instance_key:{:?}",
			instance_address, contract_instance_key
		);
		let value = serde_json::to_string(&contract_instance_value)?;
		match self.db.put(key, value) {
			Ok(_) => Ok(()),
			Err(err) => Err(anyhow!("Error updating contract_instance key-value - {}", err)),
		}
	}

	async fn delete_state_key_value(
		&self,
		_instance_address: &Address,
		_key: &ContractInstanceKey,
	) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}

	async fn get_state_key_value(
		&self,
		instance_address: &Address,
		contract_instance_key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		let key = format!(
			"contract_instance_instance_address:{:?}_contract_instance_key:{:?}",
			instance_address, contract_instance_key
		);
		match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				let contract_instance_value: ContractInstanceValue =
					serde_json::from_str(&value_str).map_err(Error::new)?;
				Ok(Some(contract_instance_value))
			},
			None => Ok(None),
		}
	}

	async fn is_valid_contract_instance(&self, instance_address: &Address) -> Result<bool, Error> {
		let key =
			format!("contract_instance_contract_code_map_instance_address:{:?}", instance_address);

		match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				let contract_instance: Result<ContractInstance, Error> =
					serde_json::from_str(&value_str).map_err(Error::new);

				match contract_instance {
					Ok(_) => Ok(true),   // Contract instance found, return true
					Err(_) => Ok(false), // Deserialization error, return false
				}
			},
			None => Ok(false), // Contract instance not found, return false
		}
	}
}
