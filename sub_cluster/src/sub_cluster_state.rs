use crate::{state_cas::StateCas, state_pg::StatePg, state_rocks::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{base::BaseState, sub_cluster::SubClusterState as subcluster};

use primitives::Address;
use rocksdb::DB;
use std::sync::Arc;

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}

pub struct SubClusterState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> SubClusterState<'a> {
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a> = match &db_pool_conn {
			DbTxConn::POSTGRES(pg) => StateInternalImpl::StatePg(StatePg { pg }),
			DbTxConn::CASSANDRA(session) =>
				StateInternalImpl::StateCas(StateCas { session: session.clone() }),
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/sub_cluster", db_path);
				StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				})
			},
		};

		let state = SubClusterState { state: Arc::new(state) };

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

	pub async fn store_sub_cluster_address(
		&self,
		sub_cluster_address: &Address,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_sub_cluster_address(sub_cluster_address).await,
			StateInternalImpl::StatePg(s) => s.store_sub_cluster_address(sub_cluster_address).await,
			StateInternalImpl::StateCas(s) =>
				s.store_sub_cluster_address(sub_cluster_address).await,
		}
	}

	pub async fn store_sub_cluster_addresses(
		&self,
		sub_cluster_addresses: &Vec<Address>,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_sub_cluster_addresses(sub_cluster_addresses).await,
			StateInternalImpl::StatePg(s) =>
				s.store_sub_cluster_addresses(sub_cluster_addresses).await,
			StateInternalImpl::StateCas(s) =>
				s.store_sub_cluster_addresses(sub_cluster_addresses).await,
		}
	}

	pub async fn load_all_sub_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_all_sub_cluster_addresses().await,
			StateInternalImpl::StatePg(s) => s.load_all_sub_cluster_addresses().await,
			StateInternalImpl::StateCas(s) => s.load_all_sub_cluster_addresses().await,
		}
	}
}
