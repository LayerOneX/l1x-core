use super::bytes::Bytes;
use ethabi::RawLog;
use ethereum_types::{Bloom, H160, H256, U256};
use serde::{Deserialize, Serialize};

/// Log
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
	/// H160
	pub address: H160,
	/// Topics
	pub topics: Vec<H256>,
	/// Data
	pub data: Bytes,
	/// Block Hash
	#[serde(skip_serializing_if = "Option::is_none")]
	pub block_hash: Option<H256>,
	/// Block Number
	#[serde(skip_serializing_if = "Option::is_none")]
	pub block_number: Option<U256>,
	/// Transaction Hash
	#[serde(skip_serializing_if = "Option::is_none")]
	pub transaction_hash: Option<H256>,
	/// Transaction Index
	#[serde(skip_serializing_if = "Option::is_none")]
	pub transaction_index: Option<U256>,
	/// Log Index in Block
	#[serde(skip_serializing_if = "Option::is_none")]
	pub log_index: Option<U256>,
	/// Log Bloom in Block
	#[serde(skip_serializing_if = "Option::is_none")]
	pub logs_bloom: Option<Bloom>,
	/// Log Index in Transaction
	#[serde(skip_serializing_if = "Option::is_none")]
	pub transaction_log_index: Option<U256>,
	/// Whether Log Type is Removed (Geth Compatibility Field)
	#[serde(default)]
	pub removed: bool,
}

impl rlp::Encodable for Log {
	fn rlp_append(&self, s: &mut rlp::RlpStream) {
		s.begin_list(3);
		s.append(&self.address);
		s.append_list(&self.topics);
		s.append(&self.data.0);
	}
}

impl From<Log> for RawLog {
	fn from(val: Log) -> Self {
		(val.topics, val.data.into_vec()).into()
	}
}
