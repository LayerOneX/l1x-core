use crate::{
	block_header::BlockHeader, transaction::Transaction,
	transaction_receipt::TransactionReceiptResponse,
};
use anyhow::{anyhow, Error as AError};
use async_trait::async_trait;

use l1x_vrf::{
	common::{get_signature_from_bytes, SecpVRF},
	secp_vrf::KeySpace,
};
use libp2p_gossipsub::MessageId;
use primitives::*;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};
use libp2p::{request_response::{ ResponseChannel, RequestId }, PeerId};
use crate::vote_result::VoteResult;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QueryBlockMessage {
    pub block_number: BlockNumber,
	pub cluster_address: Address
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BlockPayload {
	pub block: Block,
	pub signature: SignatureBytes,
	pub verifying_key: VerifyingKeyBytes,
	// This is workaround to make the broadcast block message unique for each node
	pub sender: Address,
}

impl BlockPayload {
	pub async fn verify_signature(&self) -> Result<(), AError> {
		let signature_bytes: [u8; 64] = match self.signature.clone().try_into() {
			Ok(s) => s,
			Err(_) => return Err(anyhow!("Unable to get signature_bytes")),
		};
		let verifying_bytes: [u8; 33] = match self.verifying_key.clone().try_into() {
			Ok(v) => v,
			Err(_) => return Err(anyhow!("Unable to get verifying_bytes")),
		};

		let signature = get_signature_from_bytes(&signature_bytes)?;

		let public_key = KeySpace::public_key_from_bytes(&verifying_bytes)?;
		self.block
			.verify_with_ecdsa(&public_key, signature)
			.map_err(|e| anyhow!("BlockPayload: {}", e))
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match bincode::serialize(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}

impl fmt::Display for BlockPayload {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"BlockPayload {{ \n\tblock: {}, \n\tsignature: 0x{}, \n\tverifying_key 0x{}, \n\tsender 0x{} }}",
			self.block,
			hex::encode(&self.signature),
			hex::encode(&self.verifying_key),
			hex::encode(&self.sender)
		)
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockSignPayload {
	pub block_number: BlockNumber,
	pub parent_hash: BlockHash,
	pub block_type: BlockType,
	pub cluster_address: Address,
	pub transactions: Vec<Transaction>,
	pub timestamp: BlockTimeStamp,
	pub num_transactions: i32,
	pub block_version: u32,
	pub state_hash: BlockHash,
	pub epoch: Epoch,
}

impl From<&BlockPayload> for BlockSignPayload {
	fn from(block_payload: &BlockPayload) -> Self {
		BlockSignPayload {
			block_number: block_payload.block.block_header.block_number,
			parent_hash: block_payload.block.block_header.parent_hash,
			block_type: block_payload.block.block_header.block_type.clone(),
			cluster_address: block_payload.block.block_header.cluster_address,
			transactions: block_payload.block.transactions.clone(),
			timestamp: block_payload.block.block_header.timestamp,
			num_transactions: block_payload.block.block_header.num_transactions,
			block_version: block_payload.block.block_header.block_version,
			state_hash: block_payload.block.block_header.state_hash,
			epoch: block_payload.block.block_header.epoch,
		}
	}
}

impl From<&Block> for BlockSignPayload {
	fn from(block: &Block) -> Self {
		BlockSignPayload {
			block_number: block.block_header.block_number,
			parent_hash: block.block_header.parent_hash,
			block_type: block.block_header.block_type.clone(),
			cluster_address: block.block_header.cluster_address,
			transactions: block.transactions.clone(),
			timestamp: block.block_header.timestamp,
			num_transactions: block.block_header.num_transactions,
			block_version: block.block_header.block_version,
			state_hash: block.block_header.state_hash,
			epoch: block.block_header.epoch,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Block {
	pub block_header: BlockHeader,
	pub transactions: Vec<Transaction>,
}

impl Block {
	pub fn new(block_header: BlockHeader, transactions: Vec<Transaction>) -> Block {
		Block { block_header, transactions }
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match serde_json::to_vec(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}

	pub fn is_system_block(&self) -> bool {
		matches!(self.block_header.block_type, BlockType::SystemBlock)
	}
}

impl fmt::Display for Block {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"Block {{ block_header: {}, transactions: {:?} }}",
			self.block_header, self.transactions
		)
	}
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum L1xRequest {
    QueryBlock(QueryBlockMessage),
	QueryNodeStatus(TimeStamp),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum L1xResponse {
    QueryBlock {
		block_payload: BlockPayload,
		is_finalized: bool,
		vote_result: Option<VoteResult>,
    },
    QueryBlockError(String),
	QueryNodeStatus(String, TimeStamp),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BlockType {
	L1XTokenBlock,
	L1XContractBlock,
	XTalkTokenBlock,
	XTalkContractBlock,
	SuperBlock,
	SystemBlock,
}

impl Into<i8> for &BlockType {
	fn into(self) -> i8 {
		match self {
			BlockType::L1XTokenBlock => 0,
			BlockType::L1XContractBlock => 1,
			BlockType::XTalkTokenBlock => 2,
			BlockType::XTalkContractBlock => 3,
			BlockType::SuperBlock => 4,
			BlockType::SystemBlock => 5,
		}
	}
}

impl Into<i8> for BlockType {
	fn into(self) -> i8 {
		(&self).into()
	}
}

impl TryFrom<i8> for BlockType {
	type Error = anyhow::Error;

	fn try_from(value: i8) -> Result<Self, Self::Error> {
		(value as i16).try_into()
	}
}

impl TryFrom<i16> for BlockType {
	type Error = anyhow::Error;

	fn try_from(value: i16) -> Result<Self, Self::Error> {
		(value as i32).try_into()
	}
}

impl TryFrom<i32> for BlockType {
	type Error = anyhow::Error;

	fn try_from(value: i32) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(BlockType::L1XTokenBlock),
			1 => Ok(BlockType::L1XContractBlock),
			2 => Ok(BlockType::XTalkTokenBlock),
			3 => Ok(BlockType::XTalkContractBlock),
			4 => Ok(BlockType::SuperBlock),
			5 => Ok(BlockType::SystemBlock),
			_ => Err(anyhow!("Unknow block type idx {}", value)),
		}
	}
}

#[async_trait]
pub trait BlockBroadcast {
	async fn block_validate_broadcast(
		&self,
		block_payload: BlockPayload,
	) -> Result<MessageId, Box<dyn Error + Send>>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockResponse {
	pub block_header: BlockHeader,
	pub transactions: Vec<TransactionReceiptResponse>,
}

#[async_trait]
pub trait BlockQueryRequest {
	async fn block_query_request(
		&self,
		query: QueryBlockMessage,
		peer_id: PeerId,
	) -> Result<RequestId, Box<dyn Error + Send>>;
}

#[async_trait]
pub trait QueryBlockResponse {
	async fn block_query_response(
		&self,
		response: L1xResponse,
		channel: ResponseChannel<L1xResponse>,
	) -> Result<(), Box<dyn Error + Send>>;
}

#[async_trait]
pub trait QueryStatusRequest {
	async fn query_status_request(
		&self,
		request_time: u128,
		peer_id: PeerId,
	) -> Result<RequestId, Box<dyn Error + Send>>;
}
