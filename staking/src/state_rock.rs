use anyhow::Error;
use async_trait::async_trait;
use db_traits::{base::BaseState, staking::StakingState};
use primitives::{Address, Balance, BlockNumber};
use rocksdb::DB;
use std::fs::remove_dir_all;
use system::{staking_account::StakingAccount, staking_pool::StakingPool};
pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}
#[async_trait]
impl BaseState<StakingPool> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, _staking_pool: &StakingPool) -> Result<(), Error> {
		// Implementation for create method
		todo!()
	}

	async fn update(&self, _staking_pool: &StakingPool) -> Result<(), Error> {
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
impl StakingState for StateRock {
	async fn is_staking_account_exists(
		&self,
		_account_address: &Address,
		_pool_address: &Address,
	) -> Result<bool, Error> {
		// Implementation for set_schema_version method
		todo!()
	}
	async fn is_staking_pool_exists(&self, _pool_address: &Address) -> Result<bool, Error> {
		// Implementation for set_schema_version method
		todo!()
	}

	async fn update_staking_pool_block_number(
		&self,
		_pool_address: &Address,
		_block_number: BlockNumber,
	) -> Result<(), Error> {
		todo!()
	}

	async fn update_contract(
		&self,
		_contract_instance_address: &Address,
		_pool_address: &Address,
		_block_number: BlockNumber,
	) -> Result<(), Error> {
		todo!()
	}

	async fn get_staking_pool(&self, _pool_address: &Address) -> Result<StakingPool, Error> {
		todo!()
	}

	async fn insert_staking_account(
		&self,
		_account_address: &Address,
		_pool_address: &Address,
		_balance: Balance,
	) -> Result<(), Error> {
		todo!()
	}

	async fn update_staking_account(&self, _staking_account: &StakingAccount) -> Result<(), Error> {
		todo!()
	}

	async fn get_staking_account(
		&self,
		_account_address: &Address,
		_pool_address: &Address,
	) -> Result<StakingAccount, Error> {
		todo!()
	}

	async fn get_all_pool_stakers(
		&self,
		_pool_address: &Address,
	) -> Result<Vec<StakingAccount>, Error> {
		todo!()
	}
}
