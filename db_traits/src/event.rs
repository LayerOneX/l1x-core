use anyhow::Error;
use async_trait::async_trait;
use primitives::*;

use types::eth::{filter::Filter, log::Log};

#[async_trait]
pub trait EventState {
	async fn get_events(
		&self,
		transaction_hash: &TransactionHash,
		start_time_ms: i64, /* Pass 0 if you are calling for first time, for newer events pass
		                     * the last timestamp you received the events. */
	) -> Result<Vec<EventData>, Error>;

	async fn get_all_events(
		&self,
		transaction_hash: &TransactionHash,
	) -> Result<Vec<EventData>, Error>;

	/// Get filtered EVM events
	async fn get_filtered_events(&self, filter: Filter) -> Result<Vec<Log>, Error>;

	async fn is_valid_event(&self, transaction_hash: &TransactionHash) -> Result<bool, Error>;
}
