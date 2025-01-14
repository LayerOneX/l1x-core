use anyhow::Error;
use async_trait::async_trait;
use primitives::*;
use system::{staking_account::StakingAccount, staking_pool::StakingPool};

#[async_trait]
pub trait StakingState {
	async fn is_staking_pool_exists(&self, pool_address: &Address) -> Result<bool, Error>;

	async fn is_staking_account_exists(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<bool, Error>;
	async fn update_staking_pool_block_number(
		&self,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error>;
	async fn update_contract(
		&self,
		contract_instance_address: &Address,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error>;
	/// Returns an existing Staking Pool
	async fn get_staking_pool(&self, pool_address: &Address) -> Result<StakingPool, Error>;
	async fn insert_staking_account(
		&self,
		account_address: &Address,
		pool_address: &Address,
		balance: Balance,
	) -> Result<(), Error>;
	async fn update_staking_account(&self, staking_account: &StakingAccount) -> Result<(), Error>;
	async fn get_staking_account(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<StakingAccount, Error>;

	async fn get_all_pool_stakers(
		&self,
		pool_address: &Address,
	) -> Result<Vec<StakingAccount>, Error>;
}
