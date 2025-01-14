use crate::{state_cas::StateCas, state_pg::StatePg, state_rock::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{
	base::BaseState, contract_instance::ContractInstanceState as ContractInstanceStateInternal,
};

use primitives::*;
use rocksdb::DB;
use std::sync::Arc;
use system::contract_instance::ContractInstance;

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}
pub struct ContractInstanceState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> ContractInstanceState<'a> {
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a> = match &db_pool_conn {
			DbTxConn::POSTGRES(pg) => StateInternalImpl::StatePg(StatePg { pg }),
			DbTxConn::CASSANDRA(session) =>
				StateInternalImpl::StateCas(StateCas { session: session.clone() }),
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/contract_instance", db_path);
				StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				})
			},
		};

		let state = ContractInstanceState { state: Arc::new(state) };

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

	pub async fn store_contract_instance(
		&self,
		contract_instance: &ContractInstance,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create(contract_instance).await,
			StateInternalImpl::StatePg(s) => s.create(contract_instance).await,
			StateInternalImpl::StateCas(s) => s.create(contract_instance).await,
		}
	}

	pub async fn get_contract_instance(
		&self,
		instance_address: &Address,
	) -> Result<ContractInstance, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_contract_instance(instance_address).await,
			StateInternalImpl::StatePg(s) => s.get_contract_instance(instance_address).await,
			StateInternalImpl::StateCas(s) => s.get_contract_instance(instance_address).await,
		}
	}

	pub async fn store_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
		value: &ContractInstanceValue,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_state_key_value(instance_address, key, value).await,
			StateInternalImpl::StatePg(s) =>
				s.store_state_key_value(instance_address, key, value).await,
			StateInternalImpl::StateCas(s) =>
				s.store_state_key_value(instance_address, key, value).await,
		}
	}

	pub async fn update_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
		value: &ContractInstanceValue,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.update_state_key_value(instance_address, key, value).await,
			StateInternalImpl::StatePg(s) =>
				s.update_state_key_value(instance_address, key, value).await,
			StateInternalImpl::StateCas(s) =>
				s.update_state_key_value(instance_address, key, value).await,
		}
	}

	pub async fn delete_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.delete_state_key_value(instance_address, key).await,
			StateInternalImpl::StatePg(s) => s.delete_state_key_value(instance_address, key).await,
			StateInternalImpl::StateCas(s) => s.delete_state_key_value(instance_address, key).await,
		}
	}

	pub async fn get_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_state_key_value(instance_address, key).await,
			StateInternalImpl::StatePg(s) => s.get_state_key_value(instance_address, key).await,
			StateInternalImpl::StateCas(s) => s.get_state_key_value(instance_address, key).await,
		}
	}

	pub async fn is_valid_contract_instance(
		&self,
		instance_address: &Address,
	) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.is_valid_contract_instance(instance_address).await,
			StateInternalImpl::StatePg(s) => s.is_valid_contract_instance(instance_address).await,
			StateInternalImpl::StateCas(s) => s.is_valid_contract_instance(instance_address).await,
		}
	}
}
