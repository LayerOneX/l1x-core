use crate::{
	account::Account,
	contract::ContractType,
	transaction::{Transaction, TransactionType},
};
use anyhow::{anyhow, Error};
use primitive_types::{H160, H256};
use primitives::*;
use serde::{Deserialize, Serialize};

use sha3::{Digest, Keccak256};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionResponse {
	pub tx_hash: TransactionHash,
	pub data: Option<TransactionResponseType>,
}
use evm_runtime::CreateScheme;
impl TransactionResponse {
	/// This is used to create a new EVM address
	/// # Arguments
	/// * `scheme` - CreateScheme
	/// * `nonce` - Nonce
	/// # Returns
	/// * `H160`
	/// # Example
	/// ```
	/// use primitives::*;
	/// use evm_runtime::{CreateScheme};
	/// use system::transaction_response::TransactionResponse;
	/// let caller: Address = [1; 20];
	/// let scheme = CreateScheme::Legacy { caller: caller.into() };
	/// let nonce = 1;
	/// let address = TransactionResponse::create_evm_address(scheme, nonce);
	/// ```
	pub fn create_evm_address(scheme: CreateScheme, nonce: Nonce) -> H160 {
		match scheme {
			CreateScheme::Create2 { caller, code_hash, salt } => {
				let mut hasher = Keccak256::new();
				hasher.update([0xff]);
				hasher.update(&caller[..]);
				hasher.update(&salt[..]);
				hasher.update(&code_hash[..]);
				H256::from_slice(hasher.finalize().as_slice()).into()
			},
			CreateScheme::Legacy { caller } => {
				let mut stream = rlp::RlpStream::new_list(2);
				stream.append(&caller);
				stream.append(&nonce);
				H256::from_slice(Keccak256::digest(&stream.out()).as_slice()).into()
			},
			CreateScheme::Fixed(naddress) => naddress,
		}
	}

	/// This is used to create a new TransactionResponse from a Transaction
	/// # Arguments
	/// * `tx` - Transaction
	/// * `cluster_address` - Address of the cluster
	/// * `_contract_code` - ContractCode
	/// * `nonce` - Nonce
	/// * `account_address` - Address of the account
	/// # Returns
	/// * `Result<TransactionResponse, Error>`
	pub fn new(
		tx: Transaction,
		cluster_address: &Address,
		_contract_code: &ContractCode,
		nonce: Nonce,
		account_address: Address,
	) -> Result<Self, Error> {
		let tx_hash: TransactionHash = tx.transaction_hash()?;

		let data = match tx.transaction_type {
			TransactionType::SmartContractDeployment {
				contract_type: ContractType::EVM,
				..
			} => {
				// Deterministically calculate contract instance address
				let caller = account_address;
				let scheme = CreateScheme::Legacy { caller: caller.into() };
				let contract_instance_address = Self::create_evm_address(scheme, nonce);
				Some(TransactionResponseType::SmartContractDeployment(
					contract_instance_address.into(),
				))
			},
			TransactionType::SmartContractDeployment { .. } => {
				// Deterministically calculate address where new contract bytes will be stored
				let caller_address = Account::address(&tx.verifying_key)?;
				let contract_address =
					Account::contract_address(&caller_address, cluster_address, nonce);

				Some(TransactionResponseType::SmartContractDeployment(contract_address))
			},
			TransactionType::SmartContractInit {
				contract_code_address,
				.. 
			} => {
				let caller_address = Account::address(&tx.verifying_key)?;
				let contract_address = Account::contract_instance_address(
					&caller_address,
					&contract_code_address,
					cluster_address,
					nonce,
				);

				Some(TransactionResponseType::SmartContractInit(contract_address))
			},
			TransactionType::CreateStakingPool { .. } => {
				let caller_address = Account::address(&tx.verifying_key)?;
				let pool_address = Account::pool_address(&caller_address, cluster_address, nonce);

				Some(TransactionResponseType::CreateStakingPool(pool_address))
			},
			_ => None,
		};

		let tx_response = TransactionResponse { tx_hash, data };

		Ok(tx_response)
	}

	/// This is used to convert TransactionResponse to bytes
	/// # Arguments
	/// * `self` - TransactionResponse
	/// # Returns
	/// * `Result<Vec<u8>, Error>`
	pub fn as_bytes(&self) -> Result<Vec<u8>, Error> {
		match serde_json::to_vec(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(anyhow!("Error: {:?}", e)),
		}
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionResponseType {
	// NativeTokenTransfer(Address, Balance),
	SmartContractDeployment(Address),
	SmartContractInit(Address),
	// SmartContractFunctionCall {
	//     contract_instance_address: Address,
	//     function: ContractFunction,
	//     arguments: ContractArgument,
	// },
	XtalkSmartContractDeployment(Address),
	XtalkSmartContractInit(Address),
	// XtalkSmartContractFunctionCall {
	//     contract_instance_address: Address,
	//     function: ContractFunction,
	//     arguments: ContractArgument,
	// },
	CreateStakingPool(Address),
	// Stake {
	//     pool_address: Address,
	//     amount: Balance,
	// },
	// UnStake {
	//     pool_address: Address,
	//     amount: Balance,
	// },
}
