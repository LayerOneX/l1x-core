use primitives::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct StakingPool {
	pub pool_address: Address,
	pub pool_owner: Address,
	pub contract_instance_address: Option<Address>,
	pub cluster_address: Address,
	pub created_block_number: BlockNumber,
	pub updated_block_number: BlockNumber,
	pub min_stake: Option<Balance>,
	pub max_stake: Option<Balance>,
	pub min_pool_balance: Option<Balance>,
	pub max_pool_balance: Option<Balance>,
	pub staking_period: Option<BlockNumber>,
}

impl StakingPool {
	pub fn new(
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
	) -> StakingPool {
		StakingPool {
			pool_address: *pool_address,
			pool_owner: *pool_owner,
			contract_instance_address,
			cluster_address: *cluster_address,
			created_block_number,
			updated_block_number: created_block_number,
			min_stake,
			max_stake,
			min_pool_balance,
			max_pool_balance,
			staking_period,
		}
	}
}
