use crate::transaction::Transaction;
use primitives::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionReceiptResponse {
	pub transaction: Transaction,
	pub tx_hash: [u8; 32],
	pub status: bool,
	pub from: Address,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub fee_used: Balance,
	pub timestamp: i64,
}

impl TransactionReceiptResponse {
	pub fn new(
		transaction: Transaction,
		tx_hash: [u8; 32],
		status: bool,
		from: Address,
		block_number: BlockNumber,
		block_hash: BlockHash,
		fee_used: Balance,
		timestamp: i64,
	) -> TransactionReceiptResponse {
		TransactionReceiptResponse {
			transaction,
			tx_hash,
			status,
			from,
			block_number,
			block_hash,
			fee_used,
			timestamp,
		}
	}
}
