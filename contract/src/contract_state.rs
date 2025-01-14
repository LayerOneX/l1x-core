use crate::{state_cas::StateCas, state_pg::StatePg, state_rock::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{base::BaseState, contract::ContractState as ContractStateInternal};

use primitives::*;
use rocksdb::DB;
use std::sync::Arc;
use system::contract::Contract;

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}
pub struct ContractState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> ContractState<'a> {
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a> = match &db_pool_conn {
			DbTxConn::POSTGRES(pg) => StateInternalImpl::StatePg(StatePg { pg }),
			DbTxConn::CASSANDRA(session) =>
				StateInternalImpl::StateCas(StateCas { session: session.clone() }),
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/contract", db_path);
				StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				})
			},
		};

		let state = ContractState { state: Arc::new(state) };

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

	pub async fn store_contract(&self, contract: &Contract) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create(contract).await,
			StateInternalImpl::StatePg(s) => s.create(contract).await,
			StateInternalImpl::StateCas(s) => s.create(contract).await,
		}
	}

	pub async fn update_contract_code(&self, contract: &Contract) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.update(contract).await,
			StateInternalImpl::StatePg(s) => s.update(contract).await,
			StateInternalImpl::StateCas(s) => s.update(contract).await,
		}
	}

	pub async fn get_all_contract(&self) -> Result<Vec<Contract>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_all_contract().await,
			StateInternalImpl::StatePg(s) => s.get_all_contract().await,
			StateInternalImpl::StateCas(s) => s.get_all_contract().await,
		}
	}

	pub async fn get_contract(&self, address: &Address) -> Result<Contract, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_contract(address).await,
			StateInternalImpl::StatePg(s) => s.get_contract(address).await,
			StateInternalImpl::StateCas(s) => s.get_contract(address).await,
		}
	}

	pub async fn get_contract_owner(
		&self,
		address: &Address,
	) -> Result<(AccessType, Address), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_contract_owner(address).await,
			StateInternalImpl::StatePg(s) => s.get_contract_owner(address).await,
			StateInternalImpl::StateCas(s) => s.get_contract_owner(address).await,
		}
	}

	pub async fn is_valid_contract(&self, address: &Address) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.is_valid_contract(address).await,
			StateInternalImpl::StatePg(s) => s.is_valid_contract(address).await,
			StateInternalImpl::StateCas(s) => s.is_valid_contract(address).await,
		}
	}
}
