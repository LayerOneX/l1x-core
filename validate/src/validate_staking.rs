use account::account_state::AccountState;
use anyhow::{anyhow, Error};
use db::db::DbTxConn;
use primitives::{Address, Balance};
use staking::staking_state::StakingState;
pub struct ValidateStaking {}

impl<'a> ValidateStaking {
	/// Validates a stake transaction by checking if the pool exists, if the stake amount is over
	/// the minimum required stake, and if the account has sufficient funds to stake.
	///
	/// # Arguments
	///
	/// * `sender` - The address of the account that is staking.
	/// * `pool_address` - The address of the staking pool.
	/// * `amount` - The amount of tokens being staked.
	///
	/// # Errors
	///
	/// Returns an error if:
	///
	/// * The staked amount is less than the pool's minimum required stake.
	/// * The account has insufficient funds to stake the given amount.
	///
	/// # Returns
	///
	/// Returns `Ok(())` if the stake transaction is valid.
	pub async fn validate_stake(
		sender: &Address,
		pool_address: &Address,
		amount: Balance,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let staking_state = StakingState::new(db_pool_conn).await?;

		// Verify that the pool exists
		let staking_pool = staking_state.get_staking_pool(pool_address).await?;

		// Verify that the amount is over the minimum stake
		if let Some(min_stake) = staking_pool.min_stake {
			if amount < min_stake {
				let msg = format!(
					"Stake amount is less than the pools minimum required stake of {}",
					min_stake
				);
				return Err(anyhow!(msg))
			}
		}
		if let Some(max_stake) = staking_pool.max_stake {
			if amount > max_stake {
				let msg = format!(
					"Stake amount is greater than the pools maximal required stake of {}",
					max_stake
				);
				return Err(anyhow!(msg))
			}
		}
		let account_state = AccountState::new(db_pool_conn).await?;
		if let Some(max_pool_balance) = staking_pool.max_pool_balance {
			let pool_balance = account_state.get_balance(pool_address).await?;
			let pool_balance_after_stake = pool_balance.checked_add(amount)
				.ok_or(anyhow!("Pool balance overflow"))?;
			if pool_balance_after_stake > max_pool_balance {
				return Err(anyhow!("Reached max pool balance, balance: {}, max: {}, amount: {}", pool_balance, max_pool_balance, amount));
			}
		}

		let account_balance = account_state.get_balance(sender).await?;
		// Verify the account has sufficient funds.
		// Also don't let account drain itself to zero
		if account_balance <= amount {
			return Err(anyhow!("Insufficient balance to stake {} tokens", amount))
		}
		Ok(())
	}

	/// Validates an unstake operation for a given sender, pool address, and amount.
	///
	/// # Arguments
	///
	/// * `sender` - The address of the account initiating the unstake operation.
	/// * `pool_address` - The address of the staking pool.
	/// * `amount` - The amount of tokens to be unstaked.
	///
	/// # Errors
	///
	/// Returns an error if:
	///
	/// * The staking pool does not exist.
	/// * The account has less than `amount` tokens staked in the pool.
	///
	/// # Returns
	///
	/// Returns `Ok(())` if the unstake operation is valid.
	pub async fn validate_unstake(
		sender: &Address,
		pool_address: &Address,
		amount: Balance,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let staking_state = StakingState::new(db_pool_conn).await?;

		// Verify that the pool exists
		let _ = staking_state.get_staking_pool(pool_address).await?;

		// Verify that the account has `amount` or less staked in the pool
		let staking_account = staking_state.get_staking_account(sender, pool_address).await?;
		if staking_account.balance < amount {
			return Err(anyhow!(
				"Insufficient staked balance in the pool to un-stake {} tokens",
				amount
			))
		}

		Ok(())
	}
}
