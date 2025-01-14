use crate::{state_cas::StateCas, state_pg::StatePg, state_rock::StateRock};
use anyhow::Error;

use db::db::DbTxConn;
use db_traits::{base::BaseState, event::EventState as EventStateInternal};
use primitives::*;
use rocksdb::DB;
use std::sync::Arc;
use system::event::Event;
use types::eth::{filter::Filter, log::Log};

pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}
pub struct EventState<'a> {
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> EventState<'a> {
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a> = match &db_pool_conn {
			DbTxConn::POSTGRES(pg) => StateInternalImpl::StatePg(StatePg { pg }),
			DbTxConn::CASSANDRA(session) =>
				StateInternalImpl::StateCas(StateCas { session: session.clone() }),
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/event", db_path);
				StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				})
			},
		};

		let state = EventState { state: Arc::new(state) };

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

	pub async fn create_event(&self, event: &Event) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create(event).await,
			StateInternalImpl::StatePg(s) => s.create(event).await,
			StateInternalImpl::StateCas(s) => s.create(event).await,
		}
	}

	pub async fn get_events(
		&self,
		transaction_hash: &TransactionHash,
		start_time_ms: i64, /* Pass 0 if you are calling for first time, for newer events pass
		                     * the last timestamp you received the events. */
	) -> Result<Vec<EventData>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_events(transaction_hash, start_time_ms).await,
			StateInternalImpl::StatePg(s) => s.get_events(transaction_hash, start_time_ms).await,
			StateInternalImpl::StateCas(s) => s.get_events(transaction_hash, start_time_ms).await,
		}
	}

	pub async fn get_all_events(
		&self,
		transaction_hash: &TransactionHash,
	) -> Result<Vec<EventData>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_all_events(transaction_hash).await,
			StateInternalImpl::StatePg(s) => s.get_all_events(transaction_hash).await,
			StateInternalImpl::StateCas(s) => s.get_all_events(transaction_hash).await,
		}
	}

	/// Get filtered EVM events
	pub async fn get_filtered_events(&self, filter: Filter) -> Result<Vec<Log>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_filtered_events(filter).await,
			StateInternalImpl::StatePg(s) => s.get_filtered_events(filter).await,
			StateInternalImpl::StateCas(s) => s.get_filtered_events(filter).await,
		}
	}

	pub async fn is_valid_event(&self, transaction_hash: &TransactionHash) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.is_valid_event(transaction_hash).await,
			StateInternalImpl::StatePg(s) => s.is_valid_event(transaction_hash).await,
			StateInternalImpl::StateCas(s) => s.is_valid_event(transaction_hash).await,
		}
	}
}
