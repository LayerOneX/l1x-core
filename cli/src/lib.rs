use anyhow::{anyhow, Context, Result};
use l1x_rpc::rpc_model::{estimate_fee_request, submit_transaction_request_v2, EstimateFeeRequest, SubmitTransactionRequestV2};
use libp2p::PeerId;
use primitives::*;
use secp256k1::{Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use std::{error::Error, fs::File, io::Read};
pub mod types;

// TODO: these needed to be fixed properly
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionTypeNativeTX {
	NativeTokenTransfer(Address, String),
}
#[derive(Debug, Serialize, Deserialize)]
pub struct NativeTokenTransferPayload {
	pub nonce: Nonce,
	pub transaction_type: TransactionTypeNativeTX,
	pub fee_limit: Balance,
}

/// Functionality to both json and grpc clis

pub fn load_estimation_fee_req(
	fee_limit: Balance,
	payload_file_path: &str,
	private_key: &str,
) -> Result<EstimateFeeRequest, Box<dyn Error>> {
	let mut file = File::open(payload_file_path).with_context(|| "Failed to open payload file")?;
	let mut file_content = String::new();
	file.read_to_string(&mut file_content)
		.with_context(|| "Failed to read the file")?;
	let txn: types::Transaction = serde_json::from_str(&file_content)
		.with_context(|| "Failed to deserialize transaction payload")?;

		get_estimate_fee_req(fee_limit, txn, private_key)
}

pub fn get_estimate_fee_req(
	fee_limit: Balance,
	txn: types::Transaction,
	private_key: &str,
) -> Result<EstimateFeeRequest, Box<dyn Error>> {
	let submit_txn_req = get_submit_txn_req(txn, private_key, fee_limit, 0)?;

	let transaction_type = match submit_txn_req.transaction_type.ok_or_else(|| anyhow!("Can't generate transaction type"))? {
		submit_transaction_request_v2::TransactionType::NativeTokenTransfer(v) => {
			estimate_fee_request::TransactionType::NativeTokenTransfer(v)
		}
		submit_transaction_request_v2::TransactionType::CreateStakingPool(v) => {
			estimate_fee_request::TransactionType::CreateStakingPool(v)
		},
		submit_transaction_request_v2::TransactionType::Stake(v) => {
			estimate_fee_request::TransactionType::Stake(v)
		},
		submit_transaction_request_v2::TransactionType::Unstake(v) => {
			estimate_fee_request::TransactionType::Unstake(v)
		},
		submit_transaction_request_v2::TransactionType::SmartContractDeployment(v) => {
			estimate_fee_request::TransactionType::SmartContractDeployment(l1x_rpc::rpc_model::SmartContractDeploymentV2 {
				access_type: v.access_type,
				contract_code: v.contract_code,
				contract_type: v.contract_type,
				deposit: v.deposit,
				salt: v.salt,
			})
		},
		submit_transaction_request_v2::TransactionType::SmartContractInit(v) => {
			estimate_fee_request::TransactionType::SmartContractInit(l1x_rpc::rpc_model::SmartContractInitV2 {
				contract_code_address: v.contract_code_address,
				arguments: v.arguments,
				deposit: v.deposit,
			})
		},
		submit_transaction_request_v2::TransactionType::SmartContractFunctionCall(v) => {
			estimate_fee_request::TransactionType::SmartContractFunctionCall(l1x_rpc::rpc_model::SmartContractFunctionCallV2 {
				contract_instance_address: v.contract_instance_address,
				function_name: v.function_name,
				arguments: v.arguments,
				deposit: v.deposit,
			})
		},
	};

	Ok(EstimateFeeRequest {
		fee_limit: submit_txn_req.fee_limit,
		verifying_key: submit_txn_req.verifying_key,
		transaction_type: Some(transaction_type)
	})
}

pub fn load_submit_txn_req(
	payload_file_path: &str,
	private_key: &str,
	fee_limit: Balance,
	nonce: Nonce,
) -> Result<SubmitTransactionRequestV2, Box<dyn Error>> {
	let mut file = File::open(payload_file_path).with_context(|| "Failed to open payload file")?;
	let mut file_content = String::new();
	file.read_to_string(&mut file_content)
		.with_context(|| "Failed to read the file")?;
	let txn: types::Transaction = serde_json::from_str(&file_content)
		.with_context(|| "Failed to deserialize transaction payload")?;

	get_submit_txn_req(txn, private_key, fee_limit, nonce)
}

pub fn get_submit_txn_req(
	txn: types::Transaction,
	private_key: &str,
	fee_limit: Balance,
	nonce: Nonce,
) -> Result<SubmitTransactionRequestV2, Box<dyn Error>> {
	let secret_key = SecretKey::from_slice(&hex::decode(private_key)?)
		.with_context(|| "Failed to parse provided private_key")?;
	let secp = Secp256k1::new();
	let verifying_key = secret_key.public_key(&secp);

	let txn_type: l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType =
		txn.clone().try_into()?;
	
	Ok(SubmitTransactionRequestV2 {
		nonce: nonce.to_string(),
		fee_limit: fee_limit.to_string(),
		signature: l1x_rpc::sign_v2(secret_key, txn_type.clone(), fee_limit, nonce)?,
		verifying_key: verifying_key.serialize().to_vec(),
		transaction_type: Some(txn_type),
	})
}

// todo!: Remove deprecated version
#[allow(deprecated)]
pub fn secp256k1_creds(
	privkey: Option<String>,
) -> Result<(String, String, PeerId), Box<dyn Error>> {
	let (keypair, privkey) = match privkey {
		Some(privkey) => {
			let mut keypair_bytes: Vec<u8> = hex::decode(&privkey)?;

			let keypair = libp2p::identity::secp256k1::SecretKey::from_bytes(&mut keypair_bytes)
				.map(|sk| {
					libp2p::identity::Keypair::Secp256k1(
						libp2p::identity::secp256k1::Keypair::from(sk),
					)
				})?;
			(keypair, privkey)
		},
		None => {
			let keypair = libp2p::identity::Keypair::generate_secp256k1();
			let privkey_bytes = keypair.clone().try_into_secp256k1()?.secret().to_bytes();
			let privkey = hex::encode(privkey_bytes);
			(keypair, privkey)
		},
	};

	let pubkey = match keypair.public() {
		libp2p::identity::PublicKey::Secp256k1(pubkey) => pubkey,
		_ => return Err(anyhow!("Invalid key").into()),
	};
	let pubkey_bytes = pubkey.to_bytes().to_vec();
	let pubkey = hex::encode(pubkey_bytes);
	let peer_id = keypair.public().to_peer_id();

	Ok((privkey, pubkey, peer_id))
}

pub fn read_file(payload_file_path: String) -> String {
	let mut file = File::open(payload_file_path).expect("Failed to open payload file");
	let mut file_content = String::new();
	file.read_to_string(&mut file_content).expect("Failed to read the file");

	file_content
}
