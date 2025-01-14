use anyhow::Error;
use async_trait::async_trait;
use db_traits::{base::BaseState, event::EventState};
use primitives::{EventData, TransactionHash};
use rocksdb::DB;
use std::fs::remove_dir_all;
use system::event::Event;
use types::eth::{filter::Filter, log::Log};

pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}
#[async_trait]
impl BaseState<Event> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, _event: &Event) -> Result<(), Error> {
		// Implementation for create method
		todo!()
	}

	async fn update(&self, _event: &Event) -> Result<(), Error> {
		// Implementation for update method
		todo!()
	}

	async fn raw_query(&self, _query: &str) -> Result<(), Error> {
		// Remove the database directory
		remove_dir_all(&self.db_path)?;

		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}
}

#[async_trait]
impl EventState for StateRock {
	async fn get_events(
		&self,
		_transaction_hash: &TransactionHash,
		_start_time_ms: i64, /* Pass 0 if you are calling for first time, for newer events pass
		                      * the last timestamp you received the events. */
	) -> Result<Vec<EventData>, Error> {
		// Implementation for set_schema_version method
		todo!()
	}

	async fn get_all_events(
		&self,
		_transaction_hash: &TransactionHash,
	) -> Result<Vec<EventData>, Error> {
		// Implementation for set_schema_version method
		todo!()
	}

	/// Get filtered EVM events
	async fn get_filtered_events(&self, _filter: Filter) -> Result<Vec<Log>, Error> {
		// Implementation for set_schema_version method
		todo!()
	}

	async fn is_valid_event(&self, _transaction_hash: &TransactionHash) -> Result<bool, Error> {
		// Implementation for set_schema_version method
		todo!()
	}
}
