use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, contract_instance::ContractInstanceState};
use primitives::*;
use scylla::{Session, _macro_internal::CqlValue};
use std::sync::Arc;
use system::contract_instance::ContractInstance;
pub struct StateCas {
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<ContractInstance> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS contract_instance_contract_code_map (
                    instance_address blob,
                    contract_address blob,
                    owner_address blob,
                    PRIMARY KEY (instance_address, contract_address)
                );",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(_) => return Err(anyhow!("Create table failed")),
		};
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS contract_instance (
                    instance_address blob,
                    key blob,
                    value blob,
                    PRIMARY KEY (instance_address, key)
                );",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(_) => return Err(anyhow!("Create table failed")),
		};
		Ok(())
	}

	async fn create(&self, contract_instance: &ContractInstance) -> Result<(), Error> {
		match self
			.session
			.query(
				"INSERT INTO contract_instance_contract_code_map (instance_address, contract_address, owner_address) VALUES (?, ?, ?)",
				(&contract_instance.instance_address, &contract_instance.contract_address, &contract_instance.owner_address),
			)
			.await
		{
			Ok(_) => Ok(()), // Insert successful
			Err(err) => Err(anyhow!("Error inserting contract_instance_contract_code_map - {}", err)),
		}
	}

	async fn update(&self, _u: &ContractInstance) -> Result<(), Error> {
		todo!()
	}

	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match self.session.query(query, &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed to execute raw query: {}", e)),
		};
		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		todo!()
	}
}

#[async_trait]
impl ContractInstanceState for StateCas {
	async fn get_contract_instance(
		&self,
		instance_address: &Address,
	) -> Result<ContractInstance, Error> {
		let query_result = match self
			.session
			.query(
				"SELECT * FROM contract_instance_contract_code_map WHERE instance_address = ?;",
				(&instance_address,),
			)
			.await
		{
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid instance_address")),
		};

		let (contract_address_idx, _) = query_result
			.get_column_spec("contract_address")
			.ok_or_else(|| anyhow!("No contract_address column found"))?;
		let (owner_address_idx, _) = query_result
			.get_column_spec("owner_address")
			.ok_or_else(|| anyhow!("No owner_address column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		let contract_address: Address = if let Some(row) = rows.first() {
			if let Some(contract_address_value) = &row.columns[contract_address_idx] {
				if let CqlValue::Blob(contract_address) = contract_address_value {
					contract_address.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
				} else {
					return Err(anyhow!("Unable to convert to Address type"))
				}
			} else {
				return Err(anyhow!("Unable to read contract_address column"))
			}
		} else {
			return Err(anyhow!(
				"contract_instance_state: contract_address 114 : Unable to read row"
			))
		};

		let owner_address: Address = if let Some(row) = rows.first() {
			if let Some(owner_address_value) = &row.columns[owner_address_idx] {
				if let CqlValue::Blob(owner_address) = owner_address_value {
					owner_address.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
				} else {
					return Err(anyhow!("Unable to convert to Address type"))
				}
			} else {
				return Err(anyhow!("Unable to read owner_address column"))
			}
		} else {
			return Err(anyhow!("contract_instance_state: owner_address 131:Unable to read row"))
		};

		Ok(ContractInstance {
			instance_address: *instance_address,
			contract_address,
			owner_address,
		})
	}

	async fn store_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
		value: &ContractInstanceValue,
	) -> Result<(), Error> {
		match self
			.session
			.query(
				"INSERT INTO contract_instance (instance_address, key, value) VALUES (?, ?, ?)",
				(instance_address, key, value),
			)
			.await
		{
			Ok(_) => Ok(()), // Insert successful
			Err(_) => {
				// Assuming row already exists, perform the update
				self.update_state_key_value(instance_address, key, value).await?;
				Ok(())
			},
		}
	}

	async fn update_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
		value: &ContractInstanceValue,
	) -> Result<(), Error> {
		match self
			.session
			.query(
				"UPDATE contract_instance SET value = ? WHERE instance_address = ? AND key = ?;",
				(value, instance_address, key),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(err) => Err(anyhow!("Error updating contract_instance key-value - {}", err)),
		}
	}

	async fn delete_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
	) -> Result<(), Error> {
		match self
			.session
			.query(
				"DELETE FROM contract_instance WHERE instance_address = ? AND key = ?;",
				(instance_address, key),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(err) => Err(anyhow!("Error deleting contract_instance key-value - {}", err)),
		}
	}

	async fn get_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		let query_result = match self
			.session
			.query(
				"SELECT value FROM contract_instance WHERE instance_address = ? AND key = ? ;",
				(instance_address, key),
			)
			.await
		{
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid instance_address")),
		};

		let (value_idx, _) = query_result
			.get_column_spec("value")
			.ok_or_else(|| anyhow!("No value column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		let value: ContractInstanceValue = if let Some(row) = rows.first() {
			if let Some(value_value) = &row.columns[value_idx] {
				if let CqlValue::Blob(value) = value_value {
					value.clone()
				} else {
					return Err(anyhow!("Unable to convert to ContractInstanceValue type"))
				}
			} else {
				return Ok(None)
			}
		} else {
			return Ok(None)
		};

		Ok(Some(value))
	}

	async fn is_valid_contract_instance(&self, instance_address: &Address) -> Result<bool, Error> {
		let query_result = match self
			.session
			.query(
				"SELECT COUNT(*) AS count FROM contract_instance_contract_code_map WHERE instance_address = ?;",
				(&instance_address,),
			)
			.await
		{
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid instance_address")),
		};

		let (count_idx, _) = query_result
			.get_column_spec("count")
			.ok_or_else(|| anyhow!("No count column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		if let Some(row) = rows.first() {
			if let Some(count_value) = &row.columns[count_idx] {
				if let CqlValue::BigInt(count) = count_value {
					return Ok(count > &0)
				} else {
					return Err(anyhow!("Unable to convert to Nonce type"))
				}
			}
		}
		Ok(false)
	}
}
