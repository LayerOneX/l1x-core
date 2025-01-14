use crate::{state_cas::StateCas, state_pg::StatePg, state_rock::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{base::BaseState, vote::VoteState as VoteStateInternal};
use primitives::*;
use rocksdb::DB;
use std::{collections::HashMap, sync::Arc};
use system::vote::Vote;

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}
pub struct VoteState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> VoteState<'a> {
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
				let db_path = format!("{}/vote", db_path);
				state = StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				});
			},
		}

		let state = VoteState { state: Arc::new(state) };

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

	pub async fn store_vote(&self, vote: &Vote) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create(vote).await,
			StateInternalImpl::StatePg(s) => s.create(vote).await,
			StateInternalImpl::StateCas(s) => s.create(vote).await,
		}
	}

	pub async fn load_all_votes(&self, block_hash: &BlockHash) -> Result<Option<Vec<Vote>>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_all_votes(block_hash).await,
			StateInternalImpl::StatePg(s) => s.load_all_votes(block_hash).await,
			StateInternalImpl::StateCas(s) => s.load_all_votes(block_hash).await,
		}
	}

	pub async fn load_all_votes_hashmap(
		&self,
		block_hash: &BlockHash,
	) -> Result<Option<HashMap<Address, bool>>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_all_votes_hashmap(block_hash).await,
			StateInternalImpl::StatePg(s) => s.load_all_votes_hashmap(block_hash).await,
			StateInternalImpl::StateCas(s) => s.load_all_votes_hashmap(block_hash).await,
		}
	}
}
