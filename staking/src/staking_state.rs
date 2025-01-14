use crate::{state_cas::StateCas, state_pg::StatePg, state_rock::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{base::BaseState, staking::StakingState as StakingStateInternal};

use primitives::*;
use rocksdb::DB;
use std::sync::Arc;
use system::{staking_account::StakingAccount, staking_pool::StakingPool};

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}
pub struct StakingState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> StakingState<'a> {
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a> = match &db_pool_conn {
			DbTxConn::POSTGRES(pg) => StateInternalImpl::StatePg(StatePg { pg }),
			DbTxConn::CASSANDRA(session) =>
				StateInternalImpl::StateCas(StateCas { session: session.clone() }),
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/staking", db_path);
				StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				})
			},
		};

		let state = StakingState { state: Arc::new(state) };

		state.create_table().await?;
		Ok(state)
	}

	pub async fn create_table(&self) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create_table().await,
			StateInternalImpl::StatePg(s) => s.create_table().await,
			StateInternalImpl::StateCas(s) => s.create_table().await,
		}
	}

	pub async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.raw_query(query).await,
			StateInternalImpl::StatePg(s) => s.raw_query(query).await,
			StateInternalImpl::StateCas(s) => s.raw_query(query).await,
		}
	}

	/// Creates a new Staking Pool
	pub async fn insert_staking_pool(&self, staking_pool: &StakingPool) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create(staking_pool).await,
			StateInternalImpl::StatePg(s) => s.create(staking_pool).await,
			StateInternalImpl::StateCas(s) => s.create(staking_pool).await,
		}
	}

	pub async fn update_staking_pool_block_number(
		&self,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.update_staking_pool_block_number(pool_address, block_number).await,
			StateInternalImpl::StatePg(s) =>
				s.update_staking_pool_block_number(pool_address, block_number).await,
			StateInternalImpl::StateCas(s) =>
				s.update_staking_pool_block_number(pool_address, block_number).await,
		}
	}

	pub async fn update_contract(
		&self,
		contract_instance_address: &Address,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.update_contract(contract_instance_address, pool_address, block_number).await,
			StateInternalImpl::StatePg(s) =>
				s.update_contract(contract_instance_address, pool_address, block_number).await,
			StateInternalImpl::StateCas(s) =>
				s.update_contract(contract_instance_address, pool_address, block_number).await,
		}
	}

	pub async fn is_staking_pool_exists(&self, pool_address: &Address) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.is_staking_pool_exists(pool_address).await,
			StateInternalImpl::StatePg(s) => s.is_staking_pool_exists(pool_address).await,
			StateInternalImpl::StateCas(s) => s.is_staking_pool_exists(pool_address).await,
		}
	}

	/// Returns an existing Staking Pool
	pub async fn get_staking_pool(&self, pool_address: &Address) -> Result<StakingPool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_staking_pool(pool_address).await,
			StateInternalImpl::StatePg(s) => s.get_staking_pool(pool_address).await,
			StateInternalImpl::StateCas(s) => s.get_staking_pool(pool_address).await,
		}
	}

	pub async fn is_staking_account_exists(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.is_staking_account_exists(account_address, pool_address).await,
			StateInternalImpl::StatePg(s) =>
				s.is_staking_account_exists(account_address, pool_address).await,
			StateInternalImpl::StateCas(s) =>
				s.is_staking_account_exists(account_address, pool_address).await,
		}
	}

	pub async fn insert_staking_account(
		&self,
		account_address: &Address,
		pool_address: &Address,
		balance: Balance,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.insert_staking_account(account_address, pool_address, balance).await,
			StateInternalImpl::StatePg(s) =>
				s.insert_staking_account(account_address, pool_address, balance).await,
			StateInternalImpl::StateCas(s) =>
				s.insert_staking_account(account_address, pool_address, balance).await,
		}
	}

	pub async fn update_staking_account(
		&self,
		staking_account: &StakingAccount,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.update_staking_account(staking_account).await,
			StateInternalImpl::StatePg(s) => s.update_staking_account(staking_account).await,
			StateInternalImpl::StateCas(s) => s.update_staking_account(staking_account).await,
		}
	}

	pub async fn get_staking_account(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<StakingAccount, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.get_staking_account(account_address, pool_address).await,
			StateInternalImpl::StatePg(s) =>
				s.get_staking_account(account_address, pool_address).await,
			StateInternalImpl::StateCas(s) =>
				s.get_staking_account(account_address, pool_address).await,
		}
	}

	/// Returns a vector of all staking accounts that have staked in the specified pool.
	///
	/// # Arguments
	///
	/// * `pool_address` - The address of the pool to get stakers for.
	///
	/// # Returns
	///
	/// A vector of `StakingAccount` structs representing all stakers in the specified pool.
	pub async fn get_all_pool_stakers(
		&self,
		pool_address: &Address,
	) -> Result<Vec<StakingAccount>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_all_pool_stakers(pool_address).await,
			StateInternalImpl::StatePg(s) => s.get_all_pool_stakers(pool_address).await,
			StateInternalImpl::StateCas(s) => s.get_all_pool_stakers(pool_address).await,
		}
	}
}
