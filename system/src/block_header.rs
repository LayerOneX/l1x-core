use crate::block::BlockType;
use anyhow::{anyhow, Error as AError};
use l1x_vrf::{
	common::{get_signature_from_bytes, SecpVRF},
	secp_vrf::KeySpace,
};
use primitives::*;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockHeaderPayload {
	pub block_header: BlockHeader,
	pub signature: SignatureBytes,
	pub verifying_key: VerifyingKeyBytes,
}

impl BlockHeaderPayload {
	pub async fn verify_signature(&self) -> Result<(), AError> {
		let signature_bytes: [u8; 64] = match self.signature.clone().try_into() {
			Ok(s) => s,
			Err(_) => return Err(anyhow!("Unable to get signature_bytes")),
		};
		let verifying_bytes: [u8; 32] = match self.verifying_key.clone().try_into() {
			Ok(v) => v,
			Err(_) => return Err(anyhow!("Unable to get verifying_bytes")),
		};

		let signature = get_signature_from_bytes(&signature_bytes)?;

		let public_key = KeySpace::public_key_from_bytes(&verifying_bytes)?;
		let verified = self.block_header.verify_with_ecdsa(&public_key, signature);
		if verified.is_ok() {
			Ok(())
		} else {
			Err(anyhow!("Invalid Signature"))
		}
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match serde_json::to_vec(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockHeader {
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub parent_hash: BlockHash,
	pub block_type: BlockType,
	pub cluster_address: Address,
	pub timestamp: BlockTimeStamp,
	pub num_transactions: i32,
	pub epoch: Epoch,
	pub block_version: u32,
	pub state_hash: BlockHash,
}

impl BlockHeader {
	pub fn new(
		block_number: BlockNumber,
		block_hash: BlockHash,
		parent_hash: BlockHash,
		block_type: BlockType,
		cluster_address: Address,
		timestamp: BlockTimeStamp,
		num_transactions: i32,
		block_version: u32,
		state_hash: BlockHash,
		epoch: Epoch,
	) -> BlockHeader {
		BlockHeader {
			block_number,
			block_hash,
			parent_hash,
			block_type,
			cluster_address,
			timestamp,
			num_transactions,
			block_version,
			state_hash,
			epoch,
		}
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match serde_json::to_vec(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}

impl Default for BlockHeader {
	fn default() -> Self {
		BlockHeader {
			block_number: BlockNumber::default(),
			block_hash: BlockHash::default(),
			parent_hash: BlockHash::default(),
			block_type: BlockType::L1XTokenBlock,
			cluster_address: Address::default(),
			timestamp: 0,
			num_transactions: 0,
			block_version: 0,
			state_hash: BlockHash::default(),
			epoch: Epoch::default(),
		}
	}
}

impl fmt::Display for BlockHeader {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "BlockHeader {{ block_number #{}, block_hash: 0x{}, parent_hash: 0x{}, block_type: {:?}, cluster_address: 0x{}, num_transactions: {}, block_version: {}, state_hash: {} epoch: {}}}", self.block_number, hex::encode(self.block_hash), hex::encode(self.parent_hash), self.block_type, hex::encode(self.cluster_address), self.num_transactions, self.block_version, hex::encode(self.state_hash), self.epoch)
	}
}