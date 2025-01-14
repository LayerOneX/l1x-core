use anyhow::Error;
use async_trait::async_trait;
use db_traits::{base::BaseState, contract::ContractState};
use rocksdb::DB;
use std::fs::remove_dir_all;
pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}

use primitives::{AccessType, Address};
use system::contract::Contract;

#[async_trait]
impl BaseState<Contract> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, contract: &Contract) -> Result<(), Error> {
		let key = format!("contract_address:{:?}", &contract.address);
		let value = serde_json::to_string(&contract)?;
		self.db.put(&key, &value)?;

		// store contract list
		let key = "contract_list".to_string();
		let mut contracts = self.get_all_contract().await?;
		contracts.push(contract.clone());
		let value = serde_json::to_string(&contracts)?;
		self.db.put(key, value)?;

		Ok(())
	}

	async fn update(&self, contract: &Contract) -> Result<(), Error> {
		let key = format!("contract_address:{:?}", &contract.address);
		let value = serde_json::to_string(contract)?;
		self.db.put(&key, &value)?;

		// Update contract list
		let key = "contract_list".to_string();
		let mut contracts = self.get_all_contract().await?;
		contracts.push(contract.clone());
		let value = serde_json::to_string(&contracts)?;
		self.db.put(key, value)?;

		Ok(())
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
impl ContractState for StateRock {
	async fn get_all_contract(&self) -> Result<Vec<Contract>, Error> {
		let key = "contract_list".to_string();

		let contract_list: Vec<Contract> = match self.db.get(key)? {
			Some(value) => serde_json::from_slice(&value)?,
			None => Vec::new(),
		};

		Ok(contract_list)
	}

	async fn get_contract(&self, address: &Address) -> Result<Contract, Error> {
		let key = format!("contract_address:{:?}", &address);

		let contract_result: Result<Contract, Error> = match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				serde_json::from_str(&value_str).map_err(Error::new)
			},
			None => Err(Error::msg("Contract not found")),
		};

		contract_result
	}

	async fn get_contract_owner(&self, address: &Address) -> Result<(AccessType, Address), Error> {
		let key = format!("contract_address:{:?}", &address);

		let contract: Contract = match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				serde_json::from_str(&value_str).map_err(Error::new)
			},
			None => Err(Error::msg("Contract not found")),
		}?;

		Ok((contract.access, contract.owner_address))
	}

	async fn is_valid_contract(&self, address: &Address) -> Result<bool, Error> {
		let key = format!("contract_address:{:?}", &address);

		let contract_result: Result<Contract, Error> = match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				serde_json::from_str(&value_str).map_err(Error::new)
			},
			None => Err(Error::msg("Contract not found")),
		};

		// Check if the contract was successfully deserialized
		match contract_result {
			Ok(_) => Ok(true),   // Contract found, return true
			Err(_) => Ok(false), // Contract not found, return false
		}
	}
}
