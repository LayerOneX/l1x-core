// https://github.com/polkadot-evm/frontier/blob/master/client/rpc-core/src/types/sync.rs
use ethereum_types::U256;
use serde::{Serialize, Serializer};

#[derive(Clone, Debug, Eq, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfo {
	pub starting_block: U256,
	pub current_block: U256,
	pub highest_block: U256,
	pub warp_chunks_amount: Option<U256>,
	pub warp_chunks_processed: Option<U256>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyncStatus {
	/// Info when syncing
	Info(SyncInfo),
	/// Not syncing
	None,
}

impl Serialize for SyncStatus {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			SyncStatus::Info(ref info) => info.serialize(serializer),
			SyncStatus::None => false.serialize(serializer),
		}
	}
}
