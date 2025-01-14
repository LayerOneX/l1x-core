use anyhow::Error;
use primitives::*;
use traits::l1xvm_stake_call::L1XVMStakeCallTrait;

use state::UpdatedState;
pub struct ExecuteStaking {}

impl<'a> ExecuteStaking {
	pub fn execute_native_staking_create_pool(
		&self,
		account_address: &Address,
		cluster_address: &Address,
		nonce: Nonce,
		created_block_number: BlockNumber,
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
		updated_state: &mut UpdatedState,
	) -> Result<Address, Error> {
		let pool_address = updated_state.create_pool(
			account_address,
			cluster_address,
			nonce,
			created_block_number,
			contract_instance_address,
			min_stake,
			max_stake,
			min_pool_balance,
			max_pool_balance,
			staking_period,
		)?;
		Ok(pool_address)
	}

	pub fn execute_native_staking_stake(
		&self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error> {
		updated_state
			.stake(pool_address, account_address, block_number, amount)
	}

	pub fn execute_native_staking_un_stake(
		&self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		updated_state: &mut UpdatedState
	) -> Result<(), Error> {
		updated_state
			.un_stake(pool_address, account_address, block_number, amount)
	}

	pub fn execute_native_staking_update_contract(
		&self,
		pool_address: &Address,
		contract_instance_address: &Address,
		_account_address: &Address,
		block_number: BlockNumber,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error> {
		updated_state
			.update_contract(contract_instance_address, pool_address, block_number)
	}
}

impl<'a> L1XVMStakeCallTrait<'a> for ExecuteStaking {
	fn execute_native_staking_create_pool_call(
		&self,
		account_address: &Address,
		cluster_address: &Address,
		nonce: Nonce,
		created_block_number: BlockNumber,
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
		updated_state: &mut UpdatedState,
	) -> Result<Address, Error> {
			self.execute_native_staking_create_pool(
				account_address,
				cluster_address,
				nonce,
				created_block_number,
				contract_instance_address,
				min_stake,
				max_stake,
				min_pool_balance,
				max_pool_balance,
				staking_period,
				updated_state,
			)
	}

	fn execute_native_staking_stake_call(
		&self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error> {
			self.execute_native_staking_stake(
				pool_address,
				account_address,
				block_number,
				amount,
				updated_state,
			)
	}

	fn execute_native_staking_un_stake_call(
		&self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error> {
			self.execute_native_staking_un_stake(
				pool_address,
				account_address,
				block_number,
				amount,
				updated_state,
			)
	}
}
