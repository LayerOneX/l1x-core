use anyhow::Error;
use primitives::*;
use state::UpdatedState;
pub trait L1XVMStakeCallTrait<'a> {
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
	) -> Result<Address, Error>;

	fn execute_native_staking_stake_call(
		&self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error>;

	fn execute_native_staking_un_stake_call(
		&self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error>;
}
