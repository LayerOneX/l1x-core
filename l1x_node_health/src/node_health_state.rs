use crate::{state_cas::StateCas, state_pg::StatePg, state_rocks::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{
	base::BaseState,
	node_health::NodeHealthState as NodeHealthStateInternal,
};

use primitives::*;
use rocksdb::{DBWithThreadMode, MultiThreaded, Options};
use std::sync::Arc;
use system::node_health::NodeHealth;

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}

pub struct NodeHealthState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> NodeHealthState<'a> {
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a> = match db_pool_conn {
			DbTxConn::POSTGRES(pg) => StateInternalImpl::StatePg(StatePg { pg }),
			DbTxConn::CASSANDRA(session) => StateInternalImpl::StateCas(StateCas { session: session.clone() }),
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/node_health", db_path);
				let mut options = Options::default();
				options.create_if_missing(true);

				StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DBWithThreadMode::<MultiThreaded>::open(&options, db_path)?,
				})
			},
		};

		let state = NodeHealthState { state: Arc::new(state) };
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

	pub async fn store_node_health(&self, node_health: &NodeHealth) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create(node_health).await,
			StateInternalImpl::StatePg(s) => s.create(node_health).await,
			StateInternalImpl::StateCas(s) => s.create(node_health).await,
		}
	}

	pub async fn load_node_health(&self, peer_id: &str, epoch: Epoch) -> Result<Option<NodeHealth>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_node_health(peer_id, epoch).await,
			StateInternalImpl::StatePg(s) => s.load_node_health(peer_id, epoch).await,
			StateInternalImpl::StateCas(s) => s.load_node_health(peer_id, epoch).await,
		}
	}

	pub async fn update_node_health(&self, node_health: &NodeHealth) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.update(node_health).await,
			StateInternalImpl::StatePg(s) => s.update(node_health).await,
			StateInternalImpl::StateCas(s) => s.update(node_health).await,
		}
	}

	pub async fn load_node_healths(&self, epoch: Epoch) -> Result<Vec<NodeHealth>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_node_healths(epoch).await,
			StateInternalImpl::StatePg(s) => s.load_node_healths(epoch).await,
			StateInternalImpl::StateCas(s) => s.load_node_healths(epoch).await,
		}
	}

	pub async fn upsert_node_health(&self, node_health: &NodeHealth) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.create_or_update(node_health).await,
			StateInternalImpl::StatePg(s) =>
				s.create_or_update(node_health).await,
			StateInternalImpl::StateCas(s) =>
				s.create_or_update(node_health).await,
		}
	}
}