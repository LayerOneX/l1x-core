use crate::{state_cas::StateCas, state_pg::StatePg, state_rock::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{base::BaseState, vote_result::VoteResultState as VoteResultStateInternal};
use primitives::*;
use rocksdb::DB;
use std::sync::Arc;
use system::vote_result::VoteResult;

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}
pub struct VoteResultState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> VoteResultState<'a> {
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
				let db_path = format!("{}/vote_result", db_path);
				state = StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				});
			},
		}

		let state = VoteResultState { state: Arc::new(state) };

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

	pub async fn store_vote_result(&self, vote_result: &VoteResult) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create(vote_result).await,
			StateInternalImpl::StatePg(s) => s.create(vote_result).await,
			StateInternalImpl::StateCas(s) => s.create(vote_result).await,
		}
	}

	pub async fn batch_store_vote_results(&self, vote_results: &Vec<VoteResult>) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.batch_store_vote_results(vote_results).await,
			StateInternalImpl::StatePg(s) => s.batch_store_vote_results(vote_results).await,
			StateInternalImpl::StateCas(s) => s.batch_store_vote_results(vote_results).await,
		}
	}

	pub async fn load_vote_result(&self, block_hash: &BlockHash) -> Result<VoteResult, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_vote_result(block_hash).await,
			StateInternalImpl::StatePg(s) => s.load_vote_result(block_hash).await,
			StateInternalImpl::StateCas(s) => s.load_vote_result(block_hash).await,
		}
	}
}
