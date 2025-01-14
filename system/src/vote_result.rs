use anyhow::{anyhow, Error as AError};
use async_trait::async_trait;

use l1x_vrf::{
	common::{get_signature_from_bytes, SecpVRF},
	secp_vrf::KeySpace,
};
use libp2p_gossipsub::MessageId;
use primitives::*;
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::vote::Vote;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoteResultSignPayload {
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub cluster_address: Address,
	pub vote_passed: bool,
	pub votes: Vec<Vote>,
}

impl VoteResultSignPayload {
	pub fn new(
		block_number: BlockNumber,
		block_hash: BlockHash,
		cluster_address: Address,
		vote_passed: bool,
		votes: Vec<Vote>,
	) -> VoteResultSignPayload {
		VoteResultSignPayload { block_number, block_hash, cluster_address, vote_passed, votes }
	}
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoteResult {
	pub data: VoteResultSignPayload,
	pub validator_address: Address,
	pub signature: SignatureBytes,
	pub verifying_key: VerifyingKeyBytes,
}

impl VoteResult {
	/// This is used to create a new VoteResult
	/// # Arguments
	/// * `block_number` - BlockNumber
	/// * `block_hash` - BlockHash
	/// * `cluster_address` - Address
	/// * `validator_address` - Address
	/// * `signature` - SignatureBytes
	/// * `verifying_key` - VerifyingKeyBytes
	/// * `vote_passed` - bool
	pub fn new(
		block_number: BlockNumber,
		block_hash: BlockHash,
		cluster_address: Address,
		validator_address: Address,
		signature: SignatureBytes,
		verifying_key: VerifyingKeyBytes,
		vote_passed: bool,
		votes: Vec<Vote>,
	) -> VoteResult {
		VoteResult {
			data: VoteResultSignPayload { block_number, block_hash, cluster_address, vote_passed, votes },
			validator_address,
			signature,
			verifying_key,
		}
	}

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
		let verified = self.data.verify_with_ecdsa(&public_key, signature);
		if verified.is_ok() {
			Ok(())
		} else {
			Err(anyhow!("Signature on vote result is not valid"))
		}
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match bincode::serialize(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
pub trait VoteResultBroadcast {
	async fn vote_result_broadcast(
		&self,
		vote_result: VoteResult,
	) -> Result<MessageId, Box<dyn Error + Send>>;
}
