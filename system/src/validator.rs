use std::fmt;
use libp2p_gossipsub::MessageId;
use primitives::*;
use serde::{Deserialize, Serialize};
use anyhow::{anyhow, Error as AError};
use l1x_vrf::{
	common::get_signature_from_bytes,
	secp_vrf::KeySpace,
};
use std::error::Error;
use async_trait::async_trait;
use secp256k1::{Message, SecretKey};
use secp256k1::hashes::sha256;
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ValidatorPayload {
    pub validators: Vec<Validator>,
	pub epoch: Epoch,
	pub cluster_address: Address,
    pub signature: SignatureBytes,
    pub verifying_key: VerifyingKeyBytes,
    // This is workaround to make the broadcast block message unique for each node
    pub sender: Address,
	pub timestamp: u128,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidatorSignaturePayload {
	pub validators: Vec<Validator>,
	pub sender: Address,
	pub epoch: Epoch,
	pub cluster_address: Address,
}

impl ValidatorPayload {

	// Sign the validator payload
	pub async fn generate_signature(&mut self, secret_key: &SecretKey) -> Result<(), AError> {

		let validator_payload_bytes = bincode::serialize(&ValidatorSignaturePayload{
			validators: self.validators.clone(),
			sender: self.sender,
			epoch: self.epoch,
			cluster_address: self.cluster_address,
		}).map_err(|e| anyhow!("Bincode - Unable to serialize validator payload: {}", e))?;
		
		let message_validator_payload = Message::from_hashed_data::<sha256::Hash>(&validator_payload_bytes);
		let sig_validator_payload = secret_key.sign_ecdsa(message_validator_payload);
		self.signature = sig_validator_payload.serialize_compact().to_vec();
		Ok(())
	}

	// Verify the signature of the validator payload
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

		let validator_signature_payload = bincode::serialize(&ValidatorSignaturePayload{
			validators: self.validators.clone(),
			sender: self.sender,
			epoch: self.epoch,
			cluster_address: self.cluster_address,
		}).map_err(|e| anyhow!("Bincode - Unable to serialize validator payload: {}", e))?;

		let message_validator_signature_payload = Message::from_hashed_data::<sha256::Hash>(&validator_signature_payload);
		signature.verify(&message_validator_signature_payload, &public_key)
			.map_err(|e| anyhow!("Signature is not valid: {}", e))
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match bincode::serialize(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Validator {
	pub address: Address,
	pub cluster_address: Address,
	pub epoch: Epoch,
	pub stake: Balance,
	pub xscore: f64,
}

impl Validator {
	pub fn new(
		address: Address,
		cluster_address: Address,
		epoch: Epoch,
		stake: Balance,
		xscore: f64,
	) -> Validator {
		Validator { address, cluster_address, epoch, stake, xscore }
	}
	
}

impl fmt::Display for Validator {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"Validator {{ address: 0x{}, cluster_address: 0x{}, epoch #{}, stake: {} L1X tokens, xscore: {} }}",
			hex::encode(self.address),
			hex::encode(self.cluster_address),
			self.epoch,
			self.stake,
			self.xscore
		)
	}
}

impl fmt::Debug for Validator {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Display::fmt(&self, f)
	}
}

#[async_trait]
pub trait ValidatorsBroadcast {
	async fn validators_broadcast(&self, validator_payload: ValidatorPayload) -> Result<MessageId, Box<dyn Error + Send>>;
}