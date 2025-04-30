use anyhow::{anyhow, Error as AError};
use async_trait::async_trait;
use l1x_vrf::{
	common::{get_signature_from_bytes, SecpVRF},
	secp_vrf::KeySpace,
};
use libp2p_gossipsub::MessageId;
use log::debug;
use primitives::*;
use serde::{Deserialize, Serialize};
use std::error::Error;
use secp256k1::{Message, SecretKey};
use secp256k1::hashes::sha256;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockProposerPayload {
	pub cluster_address: Address,
	pub epoch: Epoch,
	pub block_proposer_address: Address,
	pub signature: SignatureBytes,
	pub verifying_key: VerifyingKeyBytes,
	// This is a workaround to make the broadcast block_proposer message unique for each node
	pub sender: Address,
	pub timestamp: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockProposerSignaturePayload {
	pub cluster_address: Address,
	pub epoch: Epoch,
	pub block_proposer_address: Address,
	pub sender: Address,
}

impl BlockProposerPayload {

	pub async fn generate_signature(&mut self, secret_key: &SecretKey) -> Result<(), AError> {

		let block_proposer_payload_bytes = bincode::serialize(&BlockProposerSignaturePayload{
			cluster_address: self.cluster_address,
			epoch: self.epoch,
			block_proposer_address: self.block_proposer_address,
			sender: self.sender,
		}).map_err(|e| anyhow!("Bincode - Unable to serialize block proposer payload: {}", e))?;

		let message_block_proposer_payload = Message::from_hashed_data::<sha256::Hash>(&block_proposer_payload_bytes);
		let sig_block_proposer_payload = secret_key.sign_ecdsa(message_block_proposer_payload);
		self.signature = sig_block_proposer_payload.serialize_compact().to_vec();
		Ok(())
	}
	// Verify the signature of the block proposer
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
		let block_proposer_payload = bincode::serialize(&BlockProposerSignaturePayload{
			cluster_address: self.cluster_address,
			epoch: self.epoch,
			block_proposer_address: self.block_proposer_address,
			sender: self.sender,
		}).map_err(|e| anyhow!("Bincode - Unable to serialize block proposer payload: {}", e))?;
		
		let message_block_proposer_payload = Message::from_hashed_data::<sha256::Hash>(&block_proposer_payload);

		signature.verify(&message_block_proposer_payload, &public_key)
			.map_err(|e| anyhow!("BlockProposerPayload: {}", e))
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match bincode::serialize(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockProposer {
	pub cluster_address: Address,
	pub epoch: Epoch,
	pub address: Address,
}

// TODO: Fix Siging and Verifying of Block Proposer similar to BlockProposerPayload

impl BlockProposer {
	/// This is used to verify the signature of the block proposer
	/// # Arguments
	/// * `self` - BlockProposer
	/// * `public_key` - PublicKey
	/// * `signature` - Signature
	/// # Returns
	/// * `BlockProposer`
	/// # Example
	/// ```
	/// use primitives::*;
	/// use l1x_vrf::common::SecpVRF;
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// use system::block_proposer::BlockProposer;
	/// let cluster_address = Address::from([0; 20]);
	/// let epoch = 1;
	/// let address = Address::from([0; 20]);
	/// let block_proposer = BlockProposer::new(cluster_address, epoch, address);
	/// ```
	pub fn new(cluster_address: Address, epoch: Epoch, address: Address) -> Self {
		BlockProposer { cluster_address, epoch, address }
	}

	pub fn verify_signature(
		&self,
		signature: SignatureBytes,
		verifying_key: VerifyingKeyBytes,
	) -> Result<bool, AError> {
		let block_proposer = self.clone();

		let signature = match get_signature_from_bytes(signature.as_slice()) {
			Ok(sig) => sig,
			Err(err) => {
				let msg = format!("Failed to get signature from bytes {:?}", err);
				log::error!("{}", msg);
				return Err(anyhow!("{}", msg))
			},
		};

		let public_key = KeySpace::public_key_from_bytes(&verifying_key)?;

		debug!("SERVER => Received Signature: {:?}", hex::encode(signature.serialize_compact()));
		debug!("SERVER => Received Public Key: {:?}", hex::encode(public_key.serialize()));

		block_proposer
			.verify_with_ecdsa(&public_key, signature)
			.map_err(|e| anyhow!("BlockProposer: {}", e))?;
		Ok(true)
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match serde_json::to_vec(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
pub trait BlockProposerBroadcast {
	async fn block_proposer_broadcast(
		&self,
		block_proposer_payload: BlockProposerPayload,
	) -> Result<MessageId, Box<dyn Error + Send>>;
}
