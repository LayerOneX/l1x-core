/// Concepts and implementations inspired by frontier project
/// https://github.com/polkadot-evm/frontier/blob/master/client/rpc-core/src/types/transaction.rs
use super::bytes::Bytes;
use ethereum::{AccessListItem, LegacyTransactionMessage};
use ethereum_types::{H160, U256, U64};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug};

#[macro_export]
macro_rules! internal_err {
	($message:expr, $data:expr) => {
		ErrorObjectOwned::owned(400, $message, $data)
	};
	($message:expr) => {
		ErrorObjectOwned::owned(400, $message, None::<()>)
	};
}

pub enum TransactionMessage {
	Legacy(LegacyTransactionMessage),
	// EIP2930(ethereum::EIP2930TransactionMessage),
	// EIP1559(ethereum::EIP1559TransactionMessage),
}

/// Transaction request coming from RPC
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRequest {
	/// Sender
	pub from: Option<H160>,
	/// Recipient
	pub to: Option<H160>,
	/// Gas Price, legacy.
	#[serde(default)]
	pub gas_price: Option<U256>,
	/// Max BaseFeePerGas the user is willing to pay.
	#[serde(default)]
	pub max_fee_per_gas: Option<U256>,
	/// The miner's tip.
	#[serde(default)]
	pub max_priority_fee_per_gas: Option<U256>,
	/// Gas
	pub gas: Option<U256>,
	/// Value of transaction in wei
	pub value: Option<U256>,
	/// The compiled code of a contract OR the first 4 bytes of the hash of the
	/// invoked method signature and encoded parameters. For details see Ethereum Contract ABI
	#[serde(skip_serializing_if = "Option::is_none")]
	pub data: Option<Bytes>,
	/// Transaction's nonce
	pub nonce: Option<U256>,
	/// Pre-pay to warm storage access.
	#[serde(default)]
	pub access_list: Option<Vec<AccessListItem>>,
	/// EIP-2718 type
	#[serde(rename = "type")]
	pub transaction_type: Option<U256>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvmSendTxnRequest {
	pub from: H160,
	pub to: Option<H160>,
	pub gas: Option<U256>,
	pub gas_price: Option<U256>,
	pub value: Option<U256>,
	pub data: Option<Bytes>,
	pub nonce: Option<U64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvmEthCallRequest {
	/// From
	pub from: Option<H160>,
	/// To
	pub to: Option<H160>,
	/// Gas Price
	pub gas_price: Option<U256>,
	/// Gas
	pub gas: Option<U256>,
	/// Value
	pub value: Option<U256>,
	/// Data
	pub data: Option<Bytes>,
	/// Nonce
	pub nonce: Option<U256>,
}
impl From<EvmEthCallRequest> for TransactionMessage {
	fn from(req: EvmEthCallRequest) -> Self {
		TransactionMessage::Legacy(LegacyTransactionMessage {
			nonce: U256::zero(),
			gas_price: req.gas_price.unwrap_or_default(),
			gas_limit: req.gas.unwrap_or_default(),
			value: req.value.unwrap_or_default(),
			input: req.data.map(|s| s.into_vec()).unwrap_or_default(),
			action: match req.to {
				Some(to) => ethereum::TransactionAction::Call(to),
				None => ethereum::TransactionAction::Create,
			},
			chain_id: None,
		})
	}
}
impl From<TransactionRequest> for TransactionMessage {
	fn from(req: TransactionRequest) -> Self {
		TransactionMessage::Legacy(LegacyTransactionMessage {
			nonce: U256::zero(),
			gas_price: req.gas_price.unwrap_or_default(),
			gas_limit: req.gas.unwrap_or_default(),
			value: req.value.unwrap_or_default(),
			input: req.data.map(|s| s.into_vec()).unwrap_or_default(),
			action: match req.to {
				Some(to) => ethereum::TransactionAction::Call(to),
				None => ethereum::TransactionAction::Create,
			},
			chain_id: None,
		})
	}
}
