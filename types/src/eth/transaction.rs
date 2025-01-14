// Concepts and implementations inspired by frontier project
// https://github.com/polkadot-evm/frontier/blob/master/client/rpc-core/src/types/transaction.rs
use super::{bytes::Bytes, log::Log};
use crate::BuildFrom;
use ethereum::{AccessListItem, TransactionAction, TransactionV2 as EthereumTransaction};
use ethereum_types::{Bloom, H160, H256, U256, U64};
use rlp::RlpStream;
use serde::{ser::SerializeStruct, Serialize, Serializer};

/// TransactionReceipt
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionReceipt {
	/// Transaction Hash
	pub transaction_hash: Option<H256>,
	/// Transaction index
	#[serde(rename = "transactionIndex")]
	pub transaction_index: Option<U256>,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Sender
	pub from: Option<H160>,
	/// Recipient
	pub to: Option<H160>,
	/// Block number
	pub block_number: Option<U256>,
	/// Cumulative gas used
	pub cumulative_gas_used: U256,
	/// Gas used
	pub gas_used: Option<U256>,
	/// Contract address
	pub contract_address: Option<H160>,
	/// Logs
	pub logs: Vec<Log>,
	/// State Root
	// NOTE(niklasad1): EIP98 makes this optional field, if it's missing then skip serializing it
	#[serde(skip_serializing_if = "Option::is_none", rename = "root")]
	pub state_root: Option<H256>,
	/// Logs bloom
	pub logs_bloom: Bloom,
	/// Status code
	// NOTE(niklasad1): Unknown after EIP98 rules, if it's missing then skip serializing it
	#[serde(skip_serializing_if = "Option::is_none", rename = "status")]
	pub status_code: Option<U64>,
	/// Effective gas price. Pre-eip1559 this is just the gasprice. Post-eip1559 this is base fee +
	/// priority fee.
	pub effective_gas_price: U256,
	/// EIP-2718 type
	#[serde(rename = "type")]
	pub transaction_type: U256,
}

/// Transaction
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
	/// EIP-2718 transaction type
	#[serde(rename = "type")]
	pub transaction_type: U256,
	/// Hash
	pub hash: H256,
	/// Nonce
	pub nonce: U256,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Block number
	pub block_number: Option<U256>,
	/// Transaction Index
	#[serde(rename = "transactionIndex")]
	pub transaction_index: Option<U256>,
	/// Sender
	pub from: H160,
	/// Recipient
	pub to: Option<H160>,
	/// Transferred value
	pub value: U256,
	/// Gas
	pub gas: U256,
	/// Gas Price
	#[serde(skip_serializing_if = "Option::is_none")]
	pub gas_price: Option<U256>,
	/// Max BaseFeePerGas the user is willing to pay.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U256>,
	/// The miner's tip.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_priority_fee_per_gas: Option<U256>,
	/// Data
	pub input: Bytes,
	/// Creates contract
	pub creates: Option<H160>,
	/// The network id of the transaction, if any.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<U64>,
	/// Pre-pay to warm storage access.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub access_list: Option<Vec<AccessListItem>>,
	/// The parity (0 for even, 1 for odd) of the y-value of the secp256k1 signature.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub y_parity: Option<U256>,
	/// The standardised V field of the signature.
	///
	/// For backwards compatibility, `v` is optionally provided as an alternative to `yParity`.
	/// This field is DEPRECATED and all use of it should migrate to `yParity`.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub v: Option<U256>,
	/// The R field of the signature.
	pub r: U256,
	/// The S field of the signature.
	pub s: U256,
}

impl BuildFrom for Transaction {
	fn build_from(from: H160, transaction: &EthereumTransaction) -> Self {
		let hash = transaction.hash();
		match transaction {
			EthereumTransaction::Legacy(t) => Self {
				transaction_type: U256::from(0),
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from,
				to: match t.action {
					TransactionAction::Call(to) => Some(to),
					TransactionAction::Create => None,
				},
				value: t.value,
				gas: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				input: Bytes(t.input.clone()),
				creates: None,
				chain_id: t.signature.chain_id().map(U64::from),
				access_list: None,
				y_parity: None,
				v: Some(U256::from(t.signature.v())),
				r: U256::from(t.signature.r().as_bytes()),
				s: U256::from(t.signature.s().as_bytes()),
			},
			EthereumTransaction::EIP2930(t) => Self {
				transaction_type: U256::from(1),
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from,
				to: match t.action {
					TransactionAction::Call(to) => Some(to),
					TransactionAction::Create => None,
				},
				value: t.value,
				gas: t.gas_limit,
				gas_price: Some(t.gas_price),
				max_fee_per_gas: None,
				max_priority_fee_per_gas: None,
				input: Bytes(t.input.clone()),
				creates: None,
				chain_id: Some(U64::from(t.chain_id)),
				access_list: Some(t.access_list.clone()),
				y_parity: Some(U256::from(t.odd_y_parity as u8)),
				v: Some(U256::from(t.odd_y_parity as u8)),
				r: U256::from(t.r.as_bytes()),
				s: U256::from(t.s.as_bytes()),
			},
			EthereumTransaction::EIP1559(t) => Self {
				transaction_type: U256::from(2),
				hash,
				nonce: t.nonce,
				block_hash: None,
				block_number: None,
				transaction_index: None,
				from,
				to: match t.action {
					TransactionAction::Call(to) => Some(to),
					TransactionAction::Create => None,
				},
				value: t.value,
				gas: t.gas_limit,
				// If transaction is not mined yet, gas price is considered just max fee per gas.
				gas_price: Some(t.max_fee_per_gas),
				max_fee_per_gas: Some(t.max_fee_per_gas),
				max_priority_fee_per_gas: Some(t.max_priority_fee_per_gas),
				input: Bytes(t.input.clone()),
				creates: None,
				chain_id: Some(U64::from(t.chain_id)),
				access_list: Some(t.access_list.clone()),
				y_parity: Some(U256::from(t.odd_y_parity as u8)),
				v: Some(U256::from(t.odd_y_parity as u8)),
				r: U256::from(t.r.as_bytes()),
				s: U256::from(t.s.as_bytes()),
			},
		}
	}
}

impl Transaction {
	// Serializes the transaction without the signature
	pub fn rlp_encode(&self) -> Vec<u8> {
		let mut stream = RlpStream::new();
		stream.begin_list(9); // Adjust the list size based on your transaction fields
		stream.append(&self.nonce);
		stream.append(&self.gas_price.unwrap_or_default());
		stream.append(&self.gas);
		stream.append(&self.to.unwrap_or_default());
		stream.append(&self.value);
		stream.append(&self.input.0);
		// Append chain_id, 0, 0 for EIP155 compatibility, if needed
		stream.out().to_vec()
	}
}
/// Local Transaction Status
#[derive(Debug)]
pub enum TransactionStatus {
	/// Transaction is pending
	Pending,
	/// Transaction is in future part of the queue
	Future,
	/// Transaction was mined.
	Mined(Transaction),
	/// Transaction was removed from the queue, but not mined.
	Culled(Transaction),
	/// Transaction was dropped because of limit.
	Dropped(Transaction),
	/// Transaction was replaced by transaction with higher gas price.
	Replaced(Transaction, U256, H256),
	/// Transaction never got into the queue.
	Rejected(Transaction, String),
	/// Transaction is invalid.
	Invalid(Transaction),
	/// Transaction was canceled.
	Canceled(Transaction),
}

impl Serialize for TransactionStatus {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		use self::TransactionStatus::*;

		let elems = match *self {
			Pending | Future => 1,
			Mined(..) | Culled(..) | Dropped(..) | Invalid(..) | Canceled(..) => 2,
			Rejected(..) => 3,
			Replaced(..) => 4,
		};

		let status = "status";
		let transaction = "transaction";

		let mut struc = serializer.serialize_struct("LocalTransactionStatus", elems)?;
		match *self {
			Pending => struc.serialize_field(status, "pending")?,
			Future => struc.serialize_field(status, "future")?,
			Mined(ref tx) => {
				struc.serialize_field(status, "mined")?;
				struc.serialize_field(transaction, tx)?;
			},
			Culled(ref tx) => {
				struc.serialize_field(status, "culled")?;
				struc.serialize_field(transaction, tx)?;
			},
			Dropped(ref tx) => {
				struc.serialize_field(status, "dropped")?;
				struc.serialize_field(transaction, tx)?;
			},
			Canceled(ref tx) => {
				struc.serialize_field(status, "canceled")?;
				struc.serialize_field(transaction, tx)?;
			},
			Invalid(ref tx) => {
				struc.serialize_field(status, "invalid")?;
				struc.serialize_field(transaction, tx)?;
			},
			Rejected(ref tx, ref reason) => {
				struc.serialize_field(status, "rejected")?;
				struc.serialize_field(transaction, tx)?;
				struc.serialize_field("error", reason)?;
			},
			Replaced(ref tx, ref gas_price, ref hash) => {
				struc.serialize_field(status, "replaced")?;
				struc.serialize_field(transaction, tx)?;
				struc.serialize_field("hash", hash)?;
				struc.serialize_field("gasPrice", gas_price)?;
			},
		}

		struc.end()
	}
}

/// Geth-compatible output for eth_signTransaction method
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RichRawTransaction {
	/// Raw transaction RLP
	pub raw: Bytes,
	/// Transaction details
	#[serde(rename = "tx")]
	pub transaction: Transaction,
}
