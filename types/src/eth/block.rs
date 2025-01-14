// Concepts and implementations inspired by frontier project
// https://github.com/polkadot-evm/frontier/blob/master/client/rpc-core/src/types/block.rs

use super::{bytes::Bytes, transaction::Transaction};
use ethereum_types::{Bloom, H160, H256, H64, U256};
use serde::{
	de::{Error, MapAccess, Visitor},
	Deserialize, Serialize, Serializer,
};
use std::{collections::BTreeMap, fmt, ops::Deref};

/// Represents rpc api block number param.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockNumber {
	/// Hash
	Hash {
		/// block hash
		hash: H256,
		/// only return blocks part of the canon chain
		require_canonical: bool,
	},
	/// Number
	Num(u64),
	/// Latest block
	#[default]
	Latest,
	/// Earliest block (genesis)
	Earliest,
	/// Pending block (being mined)
	Pending,
	/// The most recent crypto-economically secure block.
	/// There is no difference between Ethereum's `safe` and `finalized`
	/// in Substrate finality gadget.
	Safe,
	/// The most recent crypto-economically secure block.
	Finalized,
}

impl BlockNumber {
	/// Convert block number to min block target.
	pub fn to_min_block_num(&self) -> Option<u64> {
		match *self {
			BlockNumber::Num(ref x) => Some(*x),
			BlockNumber::Earliest => Some(0),
			_ => None,
		}
	}

	pub fn gte(&self, block_number: u128) -> bool {
		match *self {
			BlockNumber::Num(x) if x as u128 >= block_number => true,
			BlockNumber::Latest => true,
			_ => false,
		}
	}

	pub fn lte(&self, block_number: u128) -> bool {
		match *self {
			BlockNumber::Num(x) if x as u128 <= block_number => true,
			BlockNumber::Earliest => true,
			_ => false,
		}
	}
}

impl<'de> Deserialize<'de> for BlockNumber {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		deserializer.deserialize_any(BlockNumberOrHashVisitor)
	}
}

struct BlockNumberOrHashVisitor;

impl<'a> Visitor<'a> for BlockNumberOrHashVisitor {
	type Value = BlockNumber;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		write!(
			formatter,
			"a block number or 'latest', 'safe', 'finalized', 'earliest' or 'pending'"
		)
	}

	fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
	where
		V: MapAccess<'a>,
	{
		let (mut require_canonical, mut block_number, mut block_hash) =
			(false, None::<u64>, None::<H256>);

		loop {
			let key_str: Option<String> = visitor.next_key()?;

			match key_str {
				Some(key) => match key.as_str() {
					"blockNumber" => {
						let value: String = visitor.next_value()?;
						if let Some(stripped) = value.strip_prefix("0x") {
							let number = u64::from_str_radix(stripped, 16).map_err(|e| {
								Error::custom(format!("Invalid block number: {}", e))
							})?;

							block_number = Some(number);
							break;
						} else {
							return Err(Error::custom(
								"Invalid block number: missing 0x prefix".to_string(),
							));
						}
					},
					"blockHash" => {
						block_hash = Some(visitor.next_value()?);
					},
					"requireCanonical" => {
						require_canonical = visitor.next_value()?;
					},
					key => return Err(Error::custom(format!("Unknown key: {}", key))),
				},
				None => break,
			};
		}

		if let Some(number) = block_number {
			return Ok(BlockNumber::Num(number));
		}

		if let Some(hash) = block_hash {
			return Ok(BlockNumber::Hash { hash, require_canonical });
		}

		Err(Error::custom("Invalid input"))
	}

	fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
	where
		E: Error,
	{
		match value {
			"latest" => Ok(BlockNumber::Latest),
			"earliest" => Ok(BlockNumber::Earliest),
			"pending" => Ok(BlockNumber::Pending),
			"safe" => Ok(BlockNumber::Safe),
			"finalized" => Ok(BlockNumber::Finalized),
			_ if value.starts_with("0x") => u64::from_str_radix(&value[2..], 16)
				.map(BlockNumber::Num)
				.map_err(|e| Error::custom(format!("Invalid block number: {}", e))),
			_ => value.parse::<u64>().map(BlockNumber::Num).map_err(|_| {
				Error::custom("Invalid block number: non-decimal or missing 0x prefix".to_string())
			}),
		}
	}

	fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
	where
		E: Error,
	{
		self.visit_str(value.as_ref())
	}

	fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
	where
		E: Error,
	{
		Ok(BlockNumber::Num(value))
	}
}

/// Block Transactions
#[derive(Debug, Clone)]
pub enum BlockTransactions {
	/// Only hashes
	Hashes(Vec<H256>),
	/// Full transactions
	Full(Vec<Transaction>),
}

impl Serialize for BlockTransactions {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match *self {
			BlockTransactions::Hashes(ref hashes) => hashes.serialize(serializer),
			BlockTransactions::Full(ref ts) => ts.serialize(serializer),
		}
	}
}

impl Default for BlockTransactions {
	fn default() -> Self {
		BlockTransactions::Hashes(Vec::new())
	}
}

impl FromIterator<Transaction> for BlockTransactions {
	fn from_iter<I: IntoIterator<Item = Transaction>>(iter: I) -> Self {
		BlockTransactions::Full(iter.into_iter().collect())
	}
}

/// Block representation
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
	/// Header of the block
	#[serde(flatten)]
	pub header: Header,
	/// Total difficulty
	pub total_difficulty: Option<U256>,
	/// Uncles' hashes
	pub uncles: Vec<H256>,
	/// Transactions
	pub transactions: BlockTransactions,
	/// Size in bytes
	pub size: Option<U256>,
	/// Base Fee for post-EIP1559 blocks.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub base_fee_per_gas: Option<U256>,
}

/// Block header representation.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
	/// Hash of the block
	pub hash: Option<H256>,
	/// Hash of the parent
	pub parent_hash: H256,
	/// Hash of the uncles
	#[serde(rename = "sha3Uncles")]
	pub uncles_hash: H256,
	/// Authors address
	pub author: H160,
	/// Alias of `author`
	pub miner: Option<H160>,
	/// State root hash
	pub state_root: H256,
	/// Transactions root hash
	pub transactions_root: H256,
	/// Transactions receipts root hash
	pub receipts_root: H256,
	/// Block number
	pub number: Option<U256>,
	/// Gas Used
	pub gas_used: U256,
	/// Gas Limit
	pub gas_limit: U256,
	/// Extra data
	pub extra_data: Bytes,
	/// Logs bloom
	pub logs_bloom: Bloom,
	/// Timestamp
	pub timestamp: U256,
	/// Difficulty
	pub difficulty: U256,
	/// Nonce
	pub nonce: Option<H64>,
	/// Size in bytes
	pub size: Option<U256>,
}

/// Block representation with additional info.
pub type RichBlock = Rich<Block>;

/// Header representation with additional info.
pub type RichHeader = Rich<Header>;

/// Value representation with additional info
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rich<T> {
	/// Standard value.
	pub inner: T,
	/// Engine-specific fields with additional description.
	/// Should be included directly to serialized block object.
	// TODO [ToDr] #[serde(skip_serializing)]
	pub extra_info: BTreeMap<String, String>,
}

impl<T> Deref for Rich<T> {
	type Target = T;
	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<T: Serialize> Serialize for Rich<T> {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		use serde::ser::Error;
		use serde_json::{to_value, Value};

		let serialized = (to_value(&self.inner), to_value(&self.extra_info));
		if let (Ok(Value::Object(mut value)), Ok(Value::Object(extras))) = serialized {
			// join two objects
			value.extend(extras);
			// and serialize
			value.serialize(serializer)
		} else {
			Err(S::Error::custom("Unserializable structures: expected objects"))
		}
	}
}
