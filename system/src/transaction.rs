use crate::{
	access::{self, AccessType},
	contract::{self, ContractType},
};
use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use jsonrpsee::tracing::warn;
use l1x_vrf::{
	common::{get_signature_from_bytes, SecpVRF},
	secp_vrf::KeySpace,
};
use libp2p_gossipsub::MessageId;
use log::debug;

use primitives::*;
use secp256k1::{
	ecdsa::{RecoveryId, Signature},
	PublicKey,
};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use ethereum_types::U256;
use super::config;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransactionVersion {
	V1 = 0,
	V2,
	V3,
}

impl Default for TransactionVersion {
	fn default() -> Self {
		TransactionVersion::V3
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionV1 {
	pub nonce: Nonce,
	pub transaction_type: TransactionType,
	pub fee_limit: Balance,
	#[serde(with = "serde_bytes")]
	pub signature: SignatureBytes,
	#[serde(with = "serde_bytes")]
	pub verifying_key: VerifyingKeyBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransactionTypeV1 {
	NativeTokenTransfer(Address, Balance),
	SmartContractDeployment {
		access_type: AccessType,
		contract_type: ContractType,
		contract_code: ContractCode,
		value: Balance,
		salt: Salt,
	},
	SmartContractInit(Address, ContractArgument),
	SmartContractFunctionCall {
		contract_instance_address: Address,
		function: ContractFunction,
		arguments: ContractArgument,
	},
	CreateStakingPool {
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
	},
	Stake {
		pool_address: Address,
		amount: Balance,
	},
	UnStake {
		pool_address: Address,
		amount: Balance,
	},
	StakingPoolContract {
		pool_address: Address,
		contract_instance_address: Address,
	},
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionV2 {
	pub nonce: Nonce,
	pub transaction_type: TransactionTypeV1,
	pub fee_limit: Balance,
	#[serde(with = "serde_bytes")]
	pub signature: SignatureBytes,
	#[serde(with = "serde_bytes")]
	pub verifying_key: VerifyingKeyBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Transaction {
	pub version: TransactionVersion,
	pub nonce: Nonce,
	pub transaction_type: TransactionType,
	pub fee_limit: Balance,
	#[serde(with = "serde_bytes")]
	pub signature: SignatureBytes,
	#[serde(with = "serde_bytes")]
	pub verifying_key: VerifyingKeyBytes,
	#[serde(with = "serde_bytes")]
	pub eth_original_transaction: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct TransactionResult {
	pub event: Option<EventData>,
	pub fee: Balance,
	pub gas_burnt: Gas,
	pub is_success: bool,
	pub error: Option<Error>
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionMetadata {
	pub transaction_hash: TransactionHash,
	pub fee: Balance,
	pub burnt_gas: Gas,
	pub is_successful: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransactionType {
	NativeTokenTransfer(Address, Balance),
	SmartContractDeployment {
		access_type: AccessType,
		contract_type: ContractType,
		contract_code: ContractCode,
		deposit: Balance,
		salt: Salt,
	},
	SmartContractInit {
		contract_code_address: Address,
		arguments: ContractArgument,
		deposit: Balance,
	},
	SmartContractFunctionCall {
		contract_instance_address: Address,
		function: ContractFunction,
		arguments: ContractArgument,
		deposit: Balance,
	},
	CreateStakingPool {
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
	},
	Stake {
		pool_address: Address,
		amount: Balance,
	},
	UnStake {
		pool_address: Address,
		amount: Balance,
	},
	StakingPoolContract {
		pool_address: Address,
		contract_instance_address: Address,
	},
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransactionTypeV1SignPayload {
	NativeTokenTransfer(Address, String),
	SmartContractDeployment {
		access_type: AccessType,
		contract_type: ContractType,
		contract_code: ContractCode,
		value: Balance,
		salt: Salt,
	},
	SmartContractInit(Address, ContractArgument),
	SmartContractFunctionCall {
		contract_instance_address: Address,
		function: ContractFunction,
		arguments: ContractArgument,
	},
	CreateStakingPool {
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
	},
	Stake {
		pool_address: Address,
		amount: Balance,
	},
	UnStake {
		pool_address: Address,
		amount: Balance,
	},
	StakingPoolContract {
		pool_address: Address,
		contract_instance_address: Address,
	},
}

impl From<TransactionType> for TransactionTypeV1SignPayload {
	fn from(value: TransactionType) -> Self {
		match value {
			TransactionType::NativeTokenTransfer(address, balance) => Self::NativeTokenTransfer(address, balance.to_string()),
			TransactionType::SmartContractDeployment { access_type, contract_type, contract_code, deposit, salt } =>
				Self::SmartContractDeployment { access_type, contract_type, contract_code, salt, value: deposit, },
			TransactionType::SmartContractFunctionCall { contract_instance_address, function, arguments, deposit: _ } =>
				Self::SmartContractFunctionCall { contract_instance_address, function, arguments },
			TransactionType::SmartContractInit { contract_code_address, arguments, deposit: _ } =>
				Self::SmartContractInit ( contract_code_address, arguments ),
			TransactionType::CreateStakingPool { contract_instance_address, min_stake, max_stake, min_pool_balance, max_pool_balance, staking_period } =>
				Self::CreateStakingPool { contract_instance_address, min_stake, max_stake, min_pool_balance, max_pool_balance, staking_period },
			TransactionType::Stake { pool_address, amount } =>
			Self::Stake { pool_address, amount },
			TransactionType::StakingPoolContract { pool_address, contract_instance_address } => 
				Self::StakingPoolContract { pool_address, contract_instance_address },
			TransactionType::UnStake { pool_address, amount } =>
				Self::UnStake { pool_address, amount }
		}
	}
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TransactionTypeV2SignPayload {
	NativeTokenTransfer(Address, String),
	SmartContractDeployment {
		access_type: AccessType,
		contract_type: ContractType,
		contract_code: ContractCode,
		deposit: String,
		salt: Salt,
	},
	SmartContractInit {
		contract_code_address: Address,
		arguments: ContractArgument,
		deposit: String,
	},
	SmartContractFunctionCall {
		contract_instance_address: Address,
		function: ContractFunction,
		arguments: ContractArgument,
		deposit: String,
	},
	CreateStakingPool {
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
	},
	Stake {
		pool_address: Address,
		amount: Balance,
	},
	UnStake {
		pool_address: Address,
		amount: Balance,
	},
	StakingPoolContract {
		pool_address: Address,
		contract_instance_address: Address,
	},
}

impl From<TransactionType> for TransactionTypeV2SignPayload {
	fn from(value: TransactionType) -> Self {
		match value {
			TransactionType::NativeTokenTransfer(address, balance) => Self::NativeTokenTransfer(address, balance.to_string()),
			TransactionType::SmartContractDeployment { access_type, contract_type, contract_code, deposit, salt } =>
			Self::SmartContractDeployment { access_type, contract_type, contract_code, deposit: deposit.to_string(), salt },
			TransactionType::SmartContractFunctionCall { contract_instance_address, function, arguments, deposit } =>
			Self::SmartContractFunctionCall { contract_instance_address, function, arguments, deposit: deposit.to_string() },
			TransactionType::SmartContractInit { contract_code_address, arguments, deposit } =>
			Self::SmartContractInit { contract_code_address, arguments, deposit: deposit.to_string() },
			TransactionType::CreateStakingPool { contract_instance_address, min_stake, max_stake, min_pool_balance, max_pool_balance, staking_period } =>
				Self::CreateStakingPool { contract_instance_address, min_stake, max_stake, min_pool_balance, max_pool_balance, staking_period },
			TransactionType::Stake { pool_address, amount } =>
				Self::Stake { pool_address, amount },
			TransactionType::StakingPoolContract { pool_address, contract_instance_address } => 
				Self::StakingPoolContract { pool_address, contract_instance_address },
			TransactionType::UnStake { pool_address, amount } =>
				Self::UnStake { pool_address, amount }
		}
	}
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TxV1SignPayload {
	pub nonce: Nonce,
	pub transaction_type: TransactionTypeV1SignPayload,
	pub fee_limit: Balance,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TxV3SignPayload {
	pub nonce: String,
	pub transaction_type: TransactionTypeV2SignPayload,
	pub fee_limit: String,
}

impl Transaction {
	/// This is used to create a new Transaction
	/// # Arguments
	/// * `nonce` - Nonce
	/// * `transaction_type` - TransactionType
	/// * `fee_limit` - Balance
	/// * `signature` - Signature
	/// * `verifying_key` - PublicKey
	/// # Returns
	/// * `Transaction`
	/// # Example
	/// ```
	/// use primitives::*;
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// use system::transaction::{Transaction, TransactionType};
	/// let nonce = 1;
	/// let transaction_type = TransactionType::NativeTokenTransfer(Address::from([0; 20]), 100);
	/// ```
	pub fn new(
		nonce: Nonce,
		transaction_type: TransactionType,
		fee_limit: Balance,
		signature: Signature,
		verifying_key: PublicKey,
	) -> Self {
		Transaction {
			version: TransactionVersion::default(),
			nonce,
			transaction_type,
			fee_limit,
			signature: signature.serialize_compact().to_vec(),
			verifying_key: verifying_key.serialize().to_vec(),
			eth_original_transaction: None,
		}
	}

	pub fn new_system(
		system_address: &Address,
		nonce: Nonce,
		transaction_type: TransactionType,
		fee_limit: Balance,
	) -> Self {
		Transaction {
			version: TransactionVersion::default(),
			nonce,
			transaction_type,
			fee_limit,
			signature: vec![],
			verifying_key: system_address.to_vec(),
			eth_original_transaction: None,
		}
	}

	pub fn new_from_eth(bytes: Vec<u8>) -> Result<Self, Error> {
		// decode bytes into ethereum TransactionV2
		let transaction: ethereum::TransactionV2 = ethereum::EnvelopedDecodable::decode(&bytes)
			.map_err(|e| anyhow!("Failed to parse log ethereum Transaction v2: {e:?}"))?;
		if let ethereum::TransactionV2::Legacy(legacy_txn) = &transaction {
			let legacy_txn2 = legacy_txn.clone();
			// convert value to primitive balance type
			let value: u128 = legacy_txn
				.value
				.try_into()
				.map_err(|e| anyhow!("Failed to convert legacy TX balance: {e}"))?;

			// create a new transaction type
			let tx_type = match legacy_txn.action {
				ethereum::TransactionAction::Call(address) => TransactionType::SmartContractFunctionCall {
					contract_instance_address: address.0,
					function: "".as_bytes().to_vec(),
					arguments: legacy_txn.input.clone(),
					deposit: value,
				},
				ethereum::TransactionAction::Create => TransactionType::SmartContractDeployment {
					access_type: access::AccessType::PUBLIC,
					contract_type: contract::ContractType::EVM,
					contract_code: legacy_txn.input.clone(),
					deposit: value,
					salt: [0; 32].to_vec(),
				},
			};

			// extract the signature and public key from the transaction
			// https://github.com/paritytech/frontier/blob/master/frame/ethereum/src/lib.rs#L373
			let (signature, pub_key): (secp256k1::ecdsa::Signature, secp256k1::PublicKey) = {
				let mut msg = [0u8; 32];
				let mut sig = [0u8; 65];
				sig[0..32].copy_from_slice(&legacy_txn.signature.r()[..]);
				sig[32..64].copy_from_slice(&legacy_txn.signature.s()[..]);
				sig[64] = legacy_txn.signature.standard_v();
				msg.copy_from_slice(
					&ethereum::LegacyTransactionMessage::from(legacy_txn2.clone()).hash()[..],
				);

				let rid =
					RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
						.map_err(|_| anyhow!("BadV"))?;
				let sig = secp256k1::ecdsa::RecoverableSignature::from_compact(&sig[..64], rid)
					.map_err(|_| anyhow!("BadRS"))?;
				let msg = secp256k1::Message::from_slice(&msg)
					.map_err(|_| anyhow!("Message is 32 bytes; qed"))?;
				let pub_key = secp256k1::SECP256K1
					.recover_ecdsa(&msg, &sig)
					.map_err(|_| anyhow!("Bad Signature"))?;
				(sig.to_standard(), pub_key)
			};
			let config = config::Config::default();
			let gas_limit = if legacy_txn.gas_limit <= U256::from(u64::MAX) {
				legacy_txn.gas_limit.as_u64()
			} else {
				return Err(anyhow!("Gas_limit overflowed"));
			};
			let transaction = Transaction {
				version: TransactionVersion::default(),
				nonce: legacy_txn.nonce.as_u128() + 1,
				transaction_type: tx_type,
				fee_limit: (gas_limit as Balance).checked_div(compile_time_config::GAS_PRICE).unwrap_or(0) + 1,
				signature: signature.serialize_compact().to_vec(),
				verifying_key: pub_key.serialize().to_vec(),
				eth_original_transaction: Some(bytes),
			};

			Ok(transaction)
		} else {
			Err(anyhow!("Transaction type not Legacy"))
		}
	}

	fn verify_eth_tx(&self) -> Result<bool, Error> {
		match Self::new_from_eth(self.eth_original_transaction.clone().unwrap()) {
			Ok(transaction) => Ok(transaction == *self),
			Err(e) => Err(e),
		}
	}

	pub fn verify_signature(&self) -> Result<bool, Error> {
		if self.eth_original_transaction.is_some() {
			return self.verify_eth_tx()
		}

		let cloned_transaction = self.clone();
		let signature = match get_signature_from_bytes(&cloned_transaction.signature.to_vec()) {
			Ok(sig) => {
				debug!("SERVER => Received Signature: {:?}", (&sig.serialize_compact()));
				sig
			},
			Err(err) => {
				let msg = format!("Failed to get signature from bytes {:?}", err);
				log::error!("{}", msg);
				return Err(anyhow!("{}", msg))
			},
		};
		
		let public_key = KeySpace::public_key_from_bytes(&self.verifying_key)?;

		debug!("SERVER => Received Signature: {:?}", hex::encode(signature.serialize_compact()));
		debug!("SERVER => Received Public Key: {:?}", hex::encode(public_key.serialize()));

		let verified = match cloned_transaction.version {
			TransactionVersion::V1
			| TransactionVersion::V2 => {
				let signed_payload = TxV1SignPayload {
					nonce: cloned_transaction.nonce,
					transaction_type: cloned_transaction.transaction_type.into(),
					fee_limit: cloned_transaction.fee_limit,
				};
				debug!("Server Signed Payload: {:?}", signed_payload);
				
				signed_payload.verify_with_ecdsa(&public_key, signature).is_ok()
			},
			TransactionVersion::V3 => {
				let signed_payload = TxV3SignPayload {
					nonce: cloned_transaction.nonce.to_string(),
					transaction_type: cloned_transaction.transaction_type.into(),
					fee_limit: cloned_transaction.fee_limit.to_string(),
				};
				debug!("Server Signed Payload: {:?}", signed_payload);
				
				signed_payload.verify_with_ecdsa(&public_key, signature).is_ok()
			}

		};

		if !verified {
			warn!("Signature on transaction is not valid");
		}
		Ok(verified)
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>> {
		match bincode::serialize(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(anyhow!("Error: {:?}", e)),
		}
	}

	pub fn transaction_hash(&self) -> Result<TransactionHash, Error> {
		if let Some(bytes) = &self.eth_original_transaction {
			// decode bytes into ethereum TransactionV2
			let transaction: ethereum::TransactionV2 = ethereum::EnvelopedDecodable::decode(&bytes)
				.map_err(|e| anyhow!("Failed to parse log ethereum Transaction v2: {e:?}"))?;
			if let ethereum::TransactionV2::Legacy(legacy_txn) = &transaction {
				// Copy hash of legacy transaction
				return Ok(legacy_txn.hash().into());
			} else {
				return Err(anyhow!("Transaction type not Legacy"));
			}
		}
		let tx_bytes = self.clone().as_bytes()?;

		// Create a Keccak-256 hasher
		let mut hasher = Keccak256::new();

		// Update the hasher with the transaction bytes
		hasher.update(&tx_bytes);

		// Obtain the hash result as a fixed-size array
		let result: TransactionHash = hasher.finalize().into();

		Ok(result)
	}

	pub fn required_balance(&self) -> Result<Balance, Error> {
		let amount = match self.transaction_type {
			TransactionType::SmartContractDeployment { deposit, .. }
			| TransactionType::SmartContractInit { deposit, .. }
			| TransactionType::SmartContractFunctionCall { deposit, .. } => deposit,
			TransactionType::NativeTokenTransfer(_, amount)
			| TransactionType::Stake { amount, .. }
			| TransactionType::UnStake { amount, .. } => amount,
			TransactionType::CreateStakingPool { .. }
			| TransactionType::StakingPoolContract { .. } => 0,
		};

		amount.checked_add(self.fee_limit).ok_or_else(|| anyhow!("Balance overflow"))
	}
}



impl From<TransactionV1> for Transaction {
	fn from(value: TransactionV1) -> Self {
		Self {
			version: TransactionVersion::V1,
			nonce: value.nonce,
			transaction_type: value.transaction_type,
			fee_limit: value.fee_limit,
			signature: value.signature,
			verifying_key: value.verifying_key,
			eth_original_transaction: None,
		}
	}
}

impl From<TransactionV2> for Transaction {
	fn from(value: TransactionV2) -> Self {
		Self {
			version: TransactionVersion::V2,
			nonce: value.nonce,
			transaction_type: value.transaction_type.into(),
			fee_limit: value.fee_limit,
			signature: value.signature,
			verifying_key: value.verifying_key,
			eth_original_transaction: None,
		}
	}
}

impl From<TransactionTypeV1> for TransactionType {
	fn from(value: TransactionTypeV1) -> Self {
		match value {
			TransactionTypeV1::NativeTokenTransfer(address, balance) => TransactionType::NativeTokenTransfer(address, balance),
			TransactionTypeV1::SmartContractDeployment { access_type, contract_type, contract_code, value, salt } => 
				TransactionType::SmartContractDeployment { access_type, contract_type, contract_code, deposit: value, salt },
			TransactionTypeV1::SmartContractInit(contract_code_address, arguments) =>
				TransactionType::SmartContractInit { contract_code_address, arguments, deposit: 0 },
			TransactionTypeV1::SmartContractFunctionCall { contract_instance_address, function, arguments } => 
				TransactionType::SmartContractFunctionCall { contract_instance_address, function, arguments, deposit: 0 },
			TransactionTypeV1::CreateStakingPool { contract_instance_address, min_stake, max_stake, min_pool_balance, max_pool_balance, staking_period } =>
				TransactionType::CreateStakingPool { contract_instance_address, min_stake, max_stake, min_pool_balance, max_pool_balance, staking_period },
			TransactionTypeV1::Stake { pool_address, amount } => TransactionType::Stake { pool_address, amount },
			TransactionTypeV1::UnStake { pool_address, amount } => TransactionType::UnStake { pool_address, amount },
			TransactionTypeV1::StakingPoolContract { pool_address, contract_instance_address } => TransactionType::StakingPoolContract { pool_address, contract_instance_address },
		}
	}
}

#[async_trait]
pub trait TransactionBroadcast {
	async fn transaction_broadcast(&self, transaction: Transaction) -> Result<MessageId>;
}
