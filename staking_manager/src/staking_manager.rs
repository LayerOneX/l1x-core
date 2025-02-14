use staking::staking_state::StakingState;
use account::{account_manager::AccountManager, account_state::AccountState};
use anyhow::{anyhow, Error};
use db::{
	db::DbTxConn,
	utils::big_int::{FromBigInt, ToBigInt},
};
use primitives::*;
use system::{account::Account, staking_pool::StakingPool};

pub struct StakingManager {}

impl<'a> StakingManager {
	pub fn new() -> StakingManager {
		StakingManager {}
	}

	pub async fn create_pool(
		&self,
		pool_address: &Address,
		pool_owner: &Address,
		contract_instance_address: Option<Address>,
		cluster_address: &Address,
		created_block_number: BlockNumber,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let pool = StakingPool::new(
			pool_address,
			pool_owner,
			contract_instance_address,
			cluster_address,
			created_block_number,
			min_stake,
			max_stake,
			min_pool_balance,
			max_pool_balance,
			staking_period,
		);
		{
			let staking_state = StakingState::new(db_pool_conn).await?;
			staking_state.insert_staking_pool(&pool.clone()).await?;
		}
		let account_state = AccountState::new(db_pool_conn).await?;
		self.create_pool_account(pool_address, &account_state).await?;
		Ok(())
	}

	/// Calls when an account un-staked from the pool
	pub async fn un_stake(
		&self,
		account_address: &Address,
		pool_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let staking_pool = {
			let staking_state = StakingState::new(db_pool_conn).await?;
			match staking_state.get_staking_pool(pool_address).await {
				Ok(res) => res,
				Err(e) => {
					let msg = format!("Un-stake: Error fetching staking pool - {}", e);
					log::error!("{}", msg);
					return Err(anyhow!(msg))
				},
			}
		};
		let staking_pool_balance = {
			let account_state = AccountState::new(db_pool_conn).await?;
			account_state.get_balance(&staking_pool.pool_address).await?
		};
		if staking_pool_balance < amount {
			let msg =
				format!("Insufficient pool balance for un-staking - {}", staking_pool_balance);
			log::error!("{}", msg);
			return Err(anyhow!(msg))
		}

		// Commented following in favor of : Hashlock Audit [M-04] staking_manager#stake - Some validators can face DoS when trying to
		// unstake due to the min pool balance condition
		
		/*
		let value_after_un_stake = staking_pool_balance
			.get_big_int()
			.checked_sub(&amount.get_big_int())
			.ok_or(anyhow!("Error Subtracting"))?;

		let min_pool_balance = if let Some(min_pool_balance) = staking_pool.min_pool_balance {
			min_pool_balance.get_big_int()
		} else {
			Zero::zero()
		};

		if value_after_un_stake.le(&min_pool_balance) {
			let msg =
				format!("Pool balance cannot be less than min pool balance - {}", min_pool_balance);
			log::error!("{}", msg);
			return Err(anyhow!(msg))
		}
		 */

		{
			let staking_state = StakingState::new(db_pool_conn).await?;
			let mut staking_account =
				staking_state.get_staking_account(account_address, pool_address).await?;
			let balance = staking_account.balance.get_big_int();
			if balance.le(&amount.get_big_int()) {
				let msg = format!(
					"Insufficient account balance for un-staking - {}",
					staking_account.balance
				);
				return Err(anyhow!(msg))
			}

			let acc_balance = staking_account
				.balance
				.get_big_int()
				.checked_sub(&amount.get_big_int())
				.ok_or(anyhow!("Error Subtracting"))?;
			staking_account.balance = u128::from_big_int(&acc_balance);

			staking_state
				.update_staking_pool_block_number(&staking_pool.pool_address, block_number)
				.await?;

			staking_state.update_staking_account(&staking_account).await?;
		}
		let account_state = AccountState::new(db_pool_conn).await?;
		self.transfer_token(pool_address, account_address, amount, &account_state)
			.await?;

		Ok(())
	}

	/// Calls when an account staked in to a pool
	/// `amount` is the amount of tokens to be staked
	/// `pool_address` is the address of the pool in which the account is staking
	/// `account_address` is the address of the account which is staking
	/// `block_number` is the block number at which the staking is happening
	pub async fn stake(
		&self,
		account_address: &Address,
		pool_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		// First, get the staking pool info
		let staking_pool = {
			let staking_state = StakingState::new(db_pool_conn).await?;
			match staking_state.get_staking_pool(pool_address).await {
				Ok(res) => res,
				Err(e) => {
					let msg = format!("Stake: Error fetching staking pool. May not exist - {}", e);
					return Err(anyhow!(msg))
				},
			}
		};
		// Then, attempt to transfer the tokens from the account to the pool
		// The database updates need to happen only if the transfer is successful
		println!(
			"Pool Address :- {:?} --- Account Address :- {:?}",
			hex::encode(pool_address),
			hex::encode(account_address)
		);
		{
			let account_state = AccountState::new(db_pool_conn).await?;
			self.transfer_token(account_address, pool_address, amount, &account_state)
				.await
				.map_err(|e| anyhow!("Error transferring token to pool - {}", e))?;
		}
		// If the transfer is successful, update the pool balance
		// Update the account balance if the account is already staking in the pool, otherwise
		// insert a new staking account record
		let staking_state = StakingState::new(db_pool_conn).await?;
		if staking_state.is_staking_account_exists(account_address, pool_address).await? {
			let mut staking_account =
				staking_state.get_staking_account(account_address, pool_address).await?;
			let balance = staking_account.balance.get_big_int();
			let acc_balance = balance
				.checked_add(&amount.get_big_int())
				.ok_or(anyhow!("Error adding new stake to existing balance. Potential overflow"))?;

			staking_account.balance = u128::from_big_int(&acc_balance);
			staking_state.update_staking_account(&staking_account).await?;
		} else {
			staking_state
				.insert_staking_account(account_address, pool_address, amount)
				.await?;
		}
		staking_state
			.update_staking_pool_block_number(&staking_pool.pool_address, block_number)
			.await?;

		Ok(())
	}

	async fn create_pool_account(
		&self,
		pool_address: &Address,
		account_state: &AccountState<'a>,
	) -> Result<(), Error> {
		let account = Account::new_system(*pool_address);
		account_state.create_account(&account).await?;
		Ok(())
	}

	async fn transfer_token(
		&self,
		from_address: &Address,
		to_address: &Address,
		amount: Balance,
		account_state: &AccountState<'a>,
	) -> Result<(), Error> {
		let account = account_state.get_account(from_address).await?;
		let mut account_manager = AccountManager { account };

		match account_manager.transfer(to_address, &amount, account_state).await {
			Ok(_) => Ok(()),
			Err(e) => {
				let msg = format!(
					"Error transferring token from {:?} to {:?} - {:?}",
					from_address, to_address, e
				);
				log::error!("{}", msg);
				Err(anyhow!(msg))
			},
		}
	}
}
