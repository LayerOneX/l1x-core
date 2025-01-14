use crate::{state_cas::StateCas, state_pg::StatePg, state_rocks::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{
	base::BaseState, block_proposer::BlockProposerState as BlockProposerStateInternal,
};

use primitives::*;
use rocksdb::{DBWithThreadMode, MultiThreaded, Options};
use std::{collections::HashMap, sync::Arc};
use system::block_proposer::BlockProposer;

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}
pub struct BlockProposerState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> BlockProposerState<'a> {
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a>;

		match &db_pool_conn {
			DbTxConn::POSTGRES(pg) => {
				state = StateInternalImpl::StatePg(StatePg { pg });
			},
			DbTxConn::CASSANDRA(session) => {
				state = StateInternalImpl::StateCas(StateCas { session: session.clone() });
			},
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/block_proposer", db_path);
				let mut options = Options::default();
				options.create_if_missing(true);

				state = StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DBWithThreadMode::<MultiThreaded>::open(&options, db_path.clone())?,
				});
			},
		}

		let state = BlockProposerState { state: Arc::new(state) };

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

	pub async fn store_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
		address: Address,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_block_proposer(cluster_address, epoch, address).await,
			StateInternalImpl::StatePg(s) =>
				s.store_block_proposer(cluster_address, epoch, address).await,
			StateInternalImpl::StateCas(s) =>
				s.store_block_proposer(cluster_address, epoch, address).await,
		}
	}

	pub async fn upsert_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
		address: Address,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.create_or_update(cluster_address, epoch, address).await,
			StateInternalImpl::StatePg(s) =>
				s.create_or_update(cluster_address, epoch, address).await,
			StateInternalImpl::StateCas(s) =>
				s.create_or_update(cluster_address, epoch, address).await,
		}
	}

	pub async fn load_selectors_block_epochs(
		&self,
		address: &Address,
	) -> Result<Option<Vec<Epoch>>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_selectors_block_epochs(address).await,
			StateInternalImpl::StatePg(s) => s.load_selectors_block_epochs(address).await,
			StateInternalImpl::StateCas(s) => s.load_selectors_block_epochs(address).await,
		}
	}

	pub async fn update_selected_next(
		&self,
		selector_address: &Address,
		cluster_address: &Address,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.update_selected_next(selector_address, cluster_address).await,
			StateInternalImpl::StatePg(s) =>
				s.update_selected_next(selector_address, cluster_address).await,
			StateInternalImpl::StateCas(s) =>
				s.update_selected_next(selector_address, cluster_address).await,
		}
	}
	pub async fn store_block_proposers(
		&self,
		block_proposers: &HashMap<Address, HashMap<Epoch, Address>>,
		selector_address: Option<Address>,
		cluster_address: Option<Address>,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_block_proposers(block_proposers, selector_address, cluster_address)
					.await,
			StateInternalImpl::StatePg(s) =>
				s.store_block_proposers(block_proposers, selector_address, cluster_address)
					.await,
			StateInternalImpl::StateCas(s) =>
				s.store_block_proposers(block_proposers, selector_address, cluster_address)
					.await,
		}
	}

	pub async fn load_last_block_proposer_epoch(
		&self,
		cluster_address: Address,
	) -> Result<Epoch, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.load_last_block_proposer_epoch(cluster_address).await,
			StateInternalImpl::StatePg(s) =>
				s.load_last_block_proposer_epoch(cluster_address).await,
			StateInternalImpl::StateCas(s) =>
				s.load_last_block_proposer_epoch(cluster_address).await,
		}
	}

	pub async fn load_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
	) -> Result<Option<BlockProposer>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.load_block_proposer(cluster_address, epoch).await,
			StateInternalImpl::StatePg(s) =>
				s.load_block_proposer(cluster_address, epoch).await,
			StateInternalImpl::StateCas(s) =>
				s.load_block_proposer(cluster_address, epoch).await,
		}
	}

	pub async fn load_block_proposers(
		&self,
		cluster_address: &Address,
	) -> Result<HashMap<Epoch, BlockProposer>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_block_proposers(cluster_address).await,
			StateInternalImpl::StatePg(s) => s.load_block_proposers(cluster_address).await,
			StateInternalImpl::StateCas(s) => s.load_block_proposers(cluster_address).await,
		}
	}

	pub async fn find_cluster_address(&self, address: Address) -> Result<Address, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.find_cluster_address(address).await,
			StateInternalImpl::StatePg(s) => s.find_cluster_address(address).await,
			StateInternalImpl::StateCas(s) => s.find_cluster_address(address).await,
		}
	}

	pub async fn is_selected_next(&self, address: &Address) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.is_selected_next(address).await,
			StateInternalImpl::StatePg(s) => s.is_selected_next(address).await,
			StateInternalImpl::StateCas(s) => s.is_selected_next(address).await,
		}
	}
}
