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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoteSignPayload {
	pub block_number: BlockNumber,
	pub epoch: Epoch,
	pub block_hash: BlockHash,
	pub cluster_address: Address,
	pub vote: bool,
}

impl VoteSignPayload {
	pub fn new(
		block_number: BlockNumber,
		block_hash: BlockHash,
		cluster_address: Address,
		epoch: Epoch,
		vote: bool,
	) -> VoteSignPayload {
		VoteSignPayload { block_number, epoch, block_hash, cluster_address, vote }
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vote {
	pub data: VoteSignPayload,
	// todo! not required
	pub validator_address: Address,
	pub signature: SignatureBytes,
	pub verifying_key: VerifyingKeyBytes,
}

impl Vote {
	pub fn new(
		data: VoteSignPayload,
		validator_address: Address,
		signature: SignatureBytes,
		verifying_key: VerifyingKeyBytes,
	) -> Vote {
		Vote { data, validator_address, signature, verifying_key }
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
		self.data
			.verify_with_ecdsa(&public_key, signature)
			.map_err(|e| anyhow!("Vote type verify_signature(): {}", e))
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match bincode::serialize(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
pub trait VoteBroadcast {
	async fn vote_broadcast(&self, vote: Vote) -> Result<MessageId, Box<dyn Error + Send>>;
}
