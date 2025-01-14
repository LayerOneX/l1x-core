use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db::utils::{FromByteArray, ToByteArray};
use db_traits::{base::BaseState, staking::StakingState};
use primitives::{arithmetic::ScalarBig, *};
use scylla::{Session, _macro_internal::CqlValue};
use std::sync::Arc;
use system::{staking_account::StakingAccount, staking_pool::StakingPool};
use util::convert::bytes_to_address;

const CREATE_TABLE_STAKING_POOL: &str = r#"
    CREATE TABLE IF NOT EXISTS  staking_pool (
        pool_address blob,
        pool_owner blob,
        contract_instance_address blob,
        cluster_address blob,
        created_block_number blob,
        updated_block_number blob,
        min_stake blob,
        max_stake blob,
        min_pool_balance blob,
        max_pool_balance blob,
        staking_period blob,
        PRIMARY KEY (pool_address)
    )
"#;

const CREATE_TABLE_STAKING_ACCOUNT: &str = r#"
    CREATE TABLE IF NOT EXISTS  staking_account (
        pool_address blob,
        account_address blob,
        balance blob,
        PRIMARY KEY (pool_address, account_address)
    )
"#;

pub struct StateCas {
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<StakingPool> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		let queries = vec![CREATE_TABLE_STAKING_POOL, CREATE_TABLE_STAKING_ACCOUNT];
		for query in queries {
			match self.session.query(query, &[]).await {
				Ok(_) => (),
				Err(e) => return Err(anyhow!("Create table failed: {}", e)),
			};
		}
		Ok(())
	}

	/// Creates a new Staking Pool
	async fn create(&self, staking_pool: &StakingPool) -> Result<(), Error> {
		let created_block_number_bytes: ScalarBig =
			staking_pool.created_block_number.to_byte_array_le(ScalarBig::default());
		let updated_block_number_bytes: ScalarBig =
			staking_pool.updated_block_number.to_byte_array_le(ScalarBig::default());
		let min_stake_bytes = match bincode::serialize(&staking_pool.min_stake) {
			Ok(res) => res,
			Err(err) => return Err(anyhow!("Failed to serialize staking_pool.min_stake {:?}", err)),
		};
		let max_stake_bytes = match bincode::serialize(&staking_pool.max_stake) {
			Ok(res) => res,
			Err(err) => return Err(anyhow!("Failed to serialize staking_pool.max_stake {:?}", err)),
		};
		let min_pool_balance_bytes = match bincode::serialize(&staking_pool.min_pool_balance) {
			Ok(res) => res,
			Err(err) =>
				return Err(anyhow!("Failed to serialize staking_pool.min_pool_balance {:?}", err)),
		};
		let max_pool_balance_bytes = match bincode::serialize(&staking_pool.max_pool_balance) {
			Ok(res) => res,
			Err(err) =>
				return Err(anyhow!("Failed to serialize staking_pool.max_pool_balance {:?}", err)),
		};
		let staking_period_bytes = match bincode::serialize(&staking_pool.staking_period) {
			Ok(res) => res,
			Err(err) =>
				return Err(anyhow!("Failed to serialize staking_pool.staking_period {:?}", err)),
		};
		let contract_instance_address_bytes =
			match bincode::serialize(&staking_pool.contract_instance_address) {
				Ok(res) => res,
				Err(err) =>
					return Err(anyhow!("Failed to serialize contract_instance_address {:?}", err)),
			};
		let query: &str = r#"
            INSERT INTO staking_pool (
                pool_address,
                pool_owner,
                contract_instance_address,
                cluster_address,
                created_block_number,
                updated_block_number,
                min_stake,
                max_stake,
                min_pool_balance,
                max_pool_balance,
                staking_period
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#;

		match self
			.session
			.query(
				query,
				(
					&staking_pool.pool_address,
					&staking_pool.pool_owner,
					&contract_instance_address_bytes,
					&staking_pool.cluster_address,
					&created_block_number_bytes,
					&updated_block_number_bytes,
					&min_stake_bytes,
					&max_stake_bytes,
					&min_pool_balance_bytes,
					&max_pool_balance_bytes,
					&staking_period_bytes,
				),
			)
			.await
		{
			Ok(_) => {},
			Err(err) => return Err(anyhow!("Failed to store staking_pool {:?}", err)),
		};
		Ok(())
	}

	async fn update(&self, _u: &StakingPool) -> Result<(), Error> {
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
impl StakingState for StateCas {
	async fn update_staking_pool_block_number(
		&self,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error> {
		let updated_block_number_bytes: ScalarBig =
			block_number.to_byte_array_le(ScalarBig::default());

		let query: &str = r#"
            UPDATE staking_pool
            SET updated_block_number = ?
            WHERE pool_address = ?
            "#;

		match self.session.query(query, (&updated_block_number_bytes, &pool_address)).await {
			Ok(_) => Ok(()),
			Err(err) => return Err(anyhow!("Failed to update_staking_pool_block_number {:?}", err)),
		}
	}

	async fn update_contract(
		&self,
		contract_instance_address: &Address,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error> {
		let updated_block_number_bytes: ScalarBig =
			block_number.to_byte_array_le(ScalarBig::default());

		let query: &str = r#"
            UPDATE staking_pool
            SET contract_instance_address = ? , updated_block_number = ?
            WHERE pool_address = ?
            "#;

		match self
			.session
			.query(query, (&contract_instance_address, &updated_block_number_bytes, &pool_address))
			.await
		{
			Ok(_) => Ok(()),
			Err(err) => return Err(anyhow!("Failed to update contract_instance_address {:?}", err)),
		}
	}

	async fn is_staking_pool_exists(&self, pool_address: &Address) -> Result<bool, Error> {
		let query: &str = r#"
            SELECT COUNT(*) AS count FROM staking_pool WHERE pool_address = ?
            "#;

		let query_result = match self.session.query(query, (&pool_address,)).await {
			Ok(q) => q,
			Err(err) => return Err(anyhow!("Failed to fetch staking pool {:?}", err)),
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

	/// Returns an existing Staking Pool
	async fn get_staking_pool(&self, pool_address: &Address) -> Result<StakingPool, Error> {
		let query: &str = r#"
            SELECT * FROM staking_pool WHERE pool_address = ?
            "#;

		let query_result = match self.session.query(query, (&pool_address,)).await {
			Ok(q) => q,
			Err(err) => return Err(anyhow!("Failed to select staking pool {:?}", err)),
		};

		let (pool_owner_idx, _) = query_result
			.get_column_spec("pool_owner")
			.ok_or_else(|| anyhow!("No pool_owner column found"))?;
		let (contract_instance_address_idx, _) = query_result
			.get_column_spec("contract_instance_address")
			.ok_or_else(|| anyhow!("No contract_instance_address column found"))?;
		let (cluster_address_idx, _) = query_result
			.get_column_spec("cluster_address")
			.ok_or_else(|| anyhow!("No cluster_address column found"))?;
		let (created_block_number_idx, _) = query_result
			.get_column_spec("created_block_number")
			.ok_or_else(|| anyhow!("No created_block_number column found"))?;
		let (updated_block_number_idx, _) = query_result
			.get_column_spec("updated_block_number")
			.ok_or_else(|| anyhow!("No updated_block_number column found"))?;
		let (min_stake_idx, _) = query_result
			.get_column_spec("min_stake")
			.ok_or_else(|| anyhow!("No min_stake column found"))?;
		let (max_stake_idx, _) = query_result
			.get_column_spec("max_stake")
			.ok_or_else(|| anyhow!("No max_stake column found"))?;
		let (min_pool_balance_idx, _) = query_result
			.get_column_spec("min_pool_balance")
			.ok_or_else(|| anyhow!("No min_pool_balance column found"))?;
		let (max_pool_balance_idx, _) = query_result
			.get_column_spec("max_pool_balance")
			.ok_or_else(|| anyhow!("No max_pool_balance column found"))?;
		let (staking_period_idx, _) = query_result
			.get_column_spec("staking_period")
			.ok_or_else(|| anyhow!("No staking_period column found"))?;

		let rows = match query_result.rows {
			Some(rows) => rows,
			None => return Err(anyhow!("Failed to get rows")),
		};

		let pool_owner: Address = if let Some(row) = rows.first() {
			if let Some(pool_owner_value) = &row.columns[pool_owner_idx] {
				if let CqlValue::Blob(pool_owner) = pool_owner_value {
					pool_owner.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
				} else {
					return Err(anyhow!("Unable to convert to Address type"))
				}
			} else {
				return Err(anyhow!("Unable to read pool_owner column"))
			}
		} else {
			return Err(anyhow!("Unable to read row"))
		};

		let contract_instance_address: Option<Address> = if let Some(row) = rows.first() {
			if let Some(contract_instance_address_value) =
				&row.columns[contract_instance_address_idx]
			{
				if let CqlValue::Blob(contract_instance_address) = contract_instance_address_value {
					bincode::deserialize(contract_instance_address)
						.map_err(|_| anyhow!("Unable to deserialize to Option<Address> type"))?
				} else {
					return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"))
				}
			} else {
				return Err(anyhow!("Unable to read contract_instance_address column"))
			}
		} else {
			return Err(anyhow!("Unable to read row"))
		};

		let cluster_address: Address = if let Some(row) = rows.first() {
			if let Some(cluster_address_value) = &row.columns[cluster_address_idx] {
				if let CqlValue::Blob(cluster_address) = cluster_address_value {
					cluster_address.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
				} else {
					return Err(anyhow!("Unable to convert to Address type"))
				}
			} else {
				return Err(anyhow!("Unable to read cluster_address column"))
			}
		} else {
			return Err(anyhow!("Unable to read row"))
		};

		let created_block_number: BlockNumber = if let Some(row) = rows.first() {
			if let Some(created_block_number_value) = &row.columns[created_block_number_idx] {
				if let CqlValue::Blob(created_block_number) = created_block_number_value {
					u128::from_byte_array(created_block_number)
				} else {
					return Err(anyhow!("Unable to convert to BlockNumber type"))
				}
			} else {
				return Err(anyhow!("Unable to read created_block_number column"))
			}
		} else {
			return Err(anyhow!("created_block_number: Unable to read row"))
		};

		let updated_block_number: BlockNumber = if let Some(row) = rows.first() {
			if let Some(updated_block_number_value) = &row.columns[updated_block_number_idx] {
				if let CqlValue::Blob(updated_block_number) = updated_block_number_value {
					u128::from_byte_array(updated_block_number)
				} else {
					return Err(anyhow!("Unable to convert to BlockNumber type"))
				}
			} else {
				return Err(anyhow!("Unable to read updated_block_number column"))
			}
		} else {
			return Err(anyhow!("updated_block_number: Unable to read row"))
		};

		let min_stake: Option<Balance> = if let Some(row) = rows.first() {
			if let Some(min_stake_value) = &row.columns[min_stake_idx] {
				if let CqlValue::Blob(min_stake) = min_stake_value {
					bincode::deserialize(min_stake)
						.map_err(|_| anyhow!("Unable to deserialize to Option<Balance> type"))?
				} else {
					return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"))
				}
			} else {
				return Err(anyhow!("Unable to read min_stake column"))
			}
		} else {
			return Err(anyhow!("min_stake: Unable to read row"))
		};

		let max_stake: Option<Balance> = if let Some(row) = rows.first() {
			if let Some(max_stake_value) = &row.columns[max_stake_idx] {
				if let CqlValue::Blob(max_stake) = max_stake_value {
					bincode::deserialize(max_stake)
						.map_err(|_| anyhow!("Unable to deserialize to Option<Balance> type"))?
				} else {
					return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"))
				}
			} else {
				return Err(anyhow!("Unable to read max_stake column"))
			}
		} else {
			return Err(anyhow!("max_stake: Unable to read row"))
		};

		let min_pool_balance: Option<Balance> = if let Some(row) = rows.first() {
			if let Some(min_pool_balance_value) = &row.columns[min_pool_balance_idx] {
				if let CqlValue::Blob(min_pool_balance) = min_pool_balance_value {
					bincode::deserialize(min_pool_balance)
						.map_err(|_| anyhow!("Unable to deserialize to Option<Balance> type"))?
				} else {
					return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"))
				}
			} else {
				return Err(anyhow!("Unable to read min_pool_balance column"))
			}
		} else {
			return Err(anyhow!("min_pool_balance: Unable to read row"))
		};

		let max_pool_balance: Option<Balance> = if let Some(row) = rows.first() {
			if let Some(max_pool_balance_value) = &row.columns[max_pool_balance_idx] {
				if let CqlValue::Blob(max_pool_balance) = max_pool_balance_value {
					bincode::deserialize(max_pool_balance)
						.map_err(|_| anyhow!("Unable to deserialize to Option<Balance> type"))?
				} else {
					return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"))
				}
			} else {
				return Err(anyhow!("Unable to read max_pool_balance column"))
			}
		} else {
			return Err(anyhow!("max_pool_balance: Unable to read row"))
		};

		let staking_period: Option<Balance> = if let Some(row) = rows.first() {
			if let Some(staking_period_value) = &row.columns[staking_period_idx] {
				if let CqlValue::Blob(staking_period) = staking_period_value {
					bincode::deserialize(staking_period)
						.map_err(|_| anyhow!("Unable to deserialize to Option<BlockNumber> type"))?
				} else {
					return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"))
				}
			} else {
				return Err(anyhow!("Unable to read staking_period column"))
			}
		} else {
			return Err(anyhow!("staking_period: Unable to read row"))
		};

		Ok(StakingPool {
			pool_address: *pool_address,
			pool_owner,
			contract_instance_address,
			cluster_address,
			created_block_number,
			updated_block_number,
			min_stake,
			max_stake,
			min_pool_balance,
			max_pool_balance,
			staking_period,
		})
	}

	async fn is_staking_account_exists(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<bool, Error> {
		let query: &str = r#"
            SELECT COUNT(*) AS count FROM staking_account WHERE account_address = ? AND pool_address = ?
            "#;

		let query_result = match self.session.query(query, (account_address, pool_address)).await {
			Ok(q) => q,
			Err(err) => return Err(anyhow!("Failed to fetch staking pool {:?}", err)),
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

	async fn insert_staking_account(
		&self,
		account_address: &Address,
		pool_address: &Address,
		balance: Balance,
	) -> Result<(), Error> {
		let balance_bytes: ScalarBig = balance.to_byte_array_le(ScalarBig::default());

		let query: &str = r#"
            INSERT INTO staking_account (
                account_address,
                pool_address,
                balance
            )
            VALUES (?, ?, ?)
            "#;

		match self.session.query(query, (account_address, pool_address, &balance_bytes)).await {
			Ok(_) => Ok(()),
			Err(err) => return Err(anyhow!("Failed to store staking account {:?}", err)),
		}
	}

	async fn update_staking_account(&self, staking_account: &StakingAccount) -> Result<(), Error> {
		let balance_bytes: ScalarBig =
			staking_account.balance.to_byte_array_le(ScalarBig::default());

		let query: &str = r#"
            UPDATE staking_account SET balance = ? WHERE account_address = ? AND pool_address = ?
            "#;
		match self
			.session
			.query(
				query,
				(&balance_bytes, &staking_account.account_address, &staking_account.pool_address),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(err) => return Err(anyhow!("Failed to store staking account {:?}", err)),
		}
	}

	async fn get_staking_account(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<StakingAccount, Error> {
		let query: &str = r#"
            SELECT balance FROM staking_account WHERE account_address = ? and pool_address = ?;
            "#;

		let query_result = match self.session.query(query, (account_address, pool_address)).await {
			Ok(q) => q,
			Err(err) => return Err(anyhow!("Failed to select staking account {:?}", err)),
		};

		let (balance_idx, _) = query_result
			.get_column_spec("balance")
			.ok_or_else(|| anyhow!("No balance column found"))?;

		let rows = match query_result.rows {
			Some(rows) => rows,
			None => return Err(anyhow!("Failed to get rows")),
		};

		let balance: Balance = if let Some(row) = rows.first() {
			if let Some(balance_value) = &row.columns[balance_idx] {
				if let CqlValue::Blob(balance) = balance_value {
					u128::from_byte_array(balance)
				} else {
					return Err(anyhow!("Unable to convert to Balance type"))
				}
			} else {
				return Err(anyhow!("Unable to read balance column"))
			}
		} else {
			return Err(anyhow!("Unable to read row"))
		};
		Ok(StakingAccount {
			account_address: *account_address,
			pool_address: *pool_address,
			balance,
		})
	}

	async fn get_all_pool_stakers(
		&self,
		pool_address: &Address,
	) -> Result<Vec<StakingAccount>, Error> {
		let query: &str = r#"
            SELECT pool_address, account_address, balance FROM staking_account WHERE pool_address = ?;
            "#;

		let query_result = match self.session.query(query, (&pool_address,)).await {
			Ok(q) => q,
			Err(err) => return Err(anyhow!("Failed to select staking account {:?}", err)),
		};

		let mut pool_stakers: Vec<StakingAccount> = Vec::new();
		if let Some(rows) = query_result.rows {
			for row in rows {
				let (pool_address, account_address, balance) =
					row.into_typed::<(Vec<u8>, Vec<u8>, Vec<u8>)>()?;

				let staking_account = StakingAccount {
					pool_address: bytes_to_address(pool_address)?,
					account_address: bytes_to_address(account_address)?,
					balance: u128::from_byte_array(balance.as_slice()),
				};

				pool_stakers.push(staking_account);
			}
		}
		// else {
		// 	return Err(anyhow!(
		// 		"No stakers found for pool 0x{}",
		// 		hex::encode(pool_address)
		// 	))
		// }

		Ok(pool_stakers)
	}
}
