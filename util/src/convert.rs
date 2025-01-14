use std::sync::Arc;
use std::u64;
use anyhow::{anyhow, Error};
use bigdecimal::BigDecimal;
use l1x_rpc::rpc_model::{self, transaction, transaction_v2, transaction_v3, NativeTokenTransfer, SmartContractDeployment,
						 SmartContractDeploymentV2, SmartContractFunctionCall, SmartContractFunctionCallV2,
						 SmartContractInit, SmartContractInitV2, Stake, Transaction, TransactionV2, TransactionV3,
						 UnStake, NodeInfo as RpcNodeInfo, NodeHealth as RpcNodeHealth, Validator as RpcValidator,
						 BlockHeaderV3, XscoreConfig as RpcXscoreConfig, StakeScoreConfig as RpcStakeScoreConfig,
						 KinScoreConfig as RpcKinScoreConfig, ValidatorRewards as RpcValidatorRewards, RuntimeStakingInfo,
						 RuntimeConfig, WbAddresses, RuntimeDenyConfig, VoteResultShort
};
use num_bigint::{BigInt, BigUint};
use num_traits::{FromPrimitive, ToPrimitive};
use primitive_types::U256;
use primitives::*;
use system::{block::BlockType, errors::NodeError};
use system::node_info::NodeInfo as SystemNodeInfo;
use system::node_health::NodeHealth as SystemNodeHealth;
use system::block_header::BlockHeader;
use system::validator::Validator as SystemValidator;
use runtime_config::{RuntimeConfigCache, RuntimeStakingInfoCache, XscoreConfig, StakeScoreConfig, KinScoreConfig, ValidatorRewards, RuntimeDenyConfigCache, WBAddresses};
use system::vote_result::{VoteResult, VoteResultSignPayload};


pub fn u256_to_balance(u256_value: &U256) -> Option<Balance> {
	// Convert the U256 to a u128 (Balance) manually.
	// Check if the U256 value can fit within a u128.
	if u256_value >= &U256::from(u128::MAX) {
		// Conversion not possible due to truncation/overflow.
		None
	} else {
		// Conversion is safe, perform the conversion.
		Some(u256_value.as_u128())
	}
}

pub fn u256_to_gas(u256_value: U256) -> Option<Gas> {
	if u256_value > U256::from(u64::MAX) {
		return None;
	}
	Some(u256_value.as_u64())
}

pub fn get_U256_value(value: Balance) -> U256 {
	let value_biguint: BigUint = BigUint::from_u128(value).expect("Convert to BigUint failed");
	let value_u256: U256 = U256::from_big_endian(&value_biguint.to_bytes_be());
	value_u256
}

/// Try to convert a vector of bytes to an address
pub fn bytes_to_address(bytes: Vec<u8>) -> Result<Address, Error> {
	if bytes.len() != 20 {
		return Err(anyhow!("Invalid address length: {}", bytes.len()));
	}
	let mut address = Address::default();
	address.copy_from_slice(&bytes);
	Ok(address)
}

/// Hex string formatted address to bytes `Address`
pub fn address_from_str(hex_str: &str) -> Result<Address, anyhow::Error> {
	// Remove the '0x' prefix if present and decode the remaining hex string
	let bytes =
		hex::decode(hex_str.trim()).map_err(|e| anyhow!("Failed to decode hex string: {}", e))?;

	if bytes.len() != 20 {
		return Err(anyhow!("Address must be 20 bytes long"));
	}

	let mut address = [0u8; 20];
	address.copy_from_slice(&bytes);
	Ok(address)
}

/// Try to convert a vector of bytes to a block hash
pub fn bytes_to_hash(bytes: Vec<u8>) -> Result<BlockHash, Error> {
	if bytes.len() != 32 {
		return Err(anyhow!("Invalid address length: {}", bytes.len()));
	}
	let mut hash = BlockHash::default();
	hash.copy_from_slice(&bytes);
	Ok(hash)
}

/// Convert from a system transaction type to a proto transaction type
pub fn to_proto_transaction(
	txn: system::transaction::Transaction,
) -> Result<Transaction, NodeError> {
	let nonce = txn.nonce.to_string();
	let fee_limit = txn.fee_limit.to_string();
	let signature = txn.signature;
	let verifying_key = txn.verifying_key;
	match txn.transaction_type {
		system::transaction::TransactionType::NativeTokenTransfer(address, balance) =>
			Ok(Transaction {
				tx_type: rpc_model::TransactionType::NativeTokenTransfer as i32,
				transaction: Some(transaction::Transaction::NativeTokenTransfer(
					NativeTokenTransfer { address: address.to_vec(), amount: balance.to_string() },
				)),
				nonce,
				fee_limit,
				signature,
				verifying_key,
			}),
		system::transaction::TransactionType::SmartContractDeployment {
			access_type,
			contract_type,
			contract_code,
			deposit,
			salt,
		} => Ok(Transaction {
			tx_type: rpc_model::TransactionType::SmartContractDeployment as i32,
			transaction: Some(transaction::Transaction::SmartContractDeployment(
				SmartContractDeployment {
					access_type: access_type as i32,
					contract_type: contract_type as i32,
					contract_code,
					value: deposit as u64,
					salt,
				},
			)),
			nonce,
			fee_limit,
			signature,
			verifying_key,
		}),
		system::transaction::TransactionType::SmartContractInit {
			contract_code_address,
			arguments,
			deposit: _,
		} => Ok(Transaction {
				tx_type: rpc_model::TransactionType::SmartContractInstantiation as i32,
				transaction: Some(transaction::Transaction::SmartContractInit(SmartContractInit {
					address: contract_code_address.to_vec(),
					arguments,
				})),
				nonce,
				fee_limit,
				signature,
				verifying_key,
			}),
		system::transaction::TransactionType::SmartContractFunctionCall {
			contract_instance_address,
			function,
			arguments,
			deposit: _,
		} => Ok(Transaction {
			tx_type: rpc_model::TransactionType::SmartContractFunctionCall as i32,
			transaction: Some(transaction::Transaction::SmartContractFunctionCall(
				SmartContractFunctionCall {
					contract_address: contract_instance_address.to_vec(),
					function_name: function,
					arguments,
				},
			)),
			nonce,
			fee_limit,
			signature,
			verifying_key,
		}),
		system::transaction::TransactionType::Stake { pool_address, amount } => Ok(Transaction {
			tx_type: rpc_model::TransactionType::Stake as i32,
			transaction: Some(transaction::Transaction::Stake(Stake {
				pool_address: pool_address.to_vec(),
				amount: amount.to_string(),
			})),
			nonce,
			fee_limit,
			signature,
			verifying_key,
		}),
		system::transaction::TransactionType::UnStake { pool_address, amount } => Ok(Transaction {
			tx_type: rpc_model::TransactionType::Unstake as i32,
			transaction: Some(transaction::Transaction::Unstake(UnStake {
				pool_address: pool_address.to_vec(),
				amount: amount.to_string(),
			})),
			nonce,
			fee_limit,
			signature,
			verifying_key,
		}),
		system::transaction::TransactionType::CreateStakingPool { .. }
		| system::transaction::TransactionType::StakingPoolContract { .. } => {
			Err(NodeError::InvalidTransactionType(format!(
				"Invalid transaction type: {:?}",
				txn.transaction_type
			)))
		},
	}
}

/// Convert from a system transaction type to a proto transaction_v2 type
pub fn to_proto_transaction_v2(
	txn: system::transaction::Transaction,
) -> Result<TransactionV2, NodeError> {
	let nonce = txn.nonce.to_string();
	let fee_limit = txn.fee_limit.to_string();
	let signature = txn.signature;
	let verifying_key = txn.verifying_key;
	let eth_original_transaction = txn.eth_original_transaction;
	match txn.transaction_type {
		system::transaction::TransactionType::NativeTokenTransfer(address, balance) =>
			Ok(TransactionV2 {
				tx_type: rpc_model::TransactionType::NativeTokenTransfer as i32,
				transaction: Some(transaction_v2::Transaction::NativeTokenTransfer(
					NativeTokenTransfer { address: address.to_vec(), amount: balance.to_string() },
				)),
				nonce,
				fee_limit,
				signature,
				verifying_key,
				eth_original_transaction,
			}),
		system::transaction::TransactionType::SmartContractDeployment {
			access_type,
			contract_type,
			contract_code,
			deposit,
			salt,
		} => Ok(TransactionV2 {
			tx_type: rpc_model::TransactionType::SmartContractDeployment as i32,
			transaction: Some(transaction_v2::Transaction::SmartContractDeployment(
				SmartContractDeployment {
					access_type: access_type as i32,
					contract_type: contract_type as i32,
					contract_code,
					value: deposit as u64,
					salt,
				},
			)),
			nonce,
			fee_limit,
			signature,
			verifying_key,
			eth_original_transaction,
		}),
		system::transaction::TransactionType::SmartContractInit {
			contract_code_address,
			arguments, 
			deposit: _
		} =>
			Ok(TransactionV2 {
				tx_type: rpc_model::TransactionType::SmartContractInstantiation as i32,
				transaction: Some(transaction_v2::Transaction::SmartContractInit(SmartContractInit {
					address: contract_code_address.to_vec(),
					arguments,
				})),
				nonce,
				fee_limit,
				signature,
				verifying_key,
				eth_original_transaction,
			}),
		system::transaction::TransactionType::SmartContractFunctionCall {
			contract_instance_address,
			function,
			arguments,
			deposit: _,
		} => Ok(TransactionV2 {
			tx_type: rpc_model::TransactionType::SmartContractFunctionCall as i32,
			transaction: Some(transaction_v2::Transaction::SmartContractFunctionCall(
				SmartContractFunctionCall {
					contract_address: contract_instance_address.to_vec(),
					function_name: function,
					arguments,
				},
			)),
			nonce,
			fee_limit,
			signature,
			verifying_key,
			eth_original_transaction,
		}),
		system::transaction::TransactionType::Stake { pool_address, amount } => Ok(TransactionV2 {
			tx_type: rpc_model::TransactionType::Stake as i32,
			transaction: Some(transaction_v2::Transaction::Stake(Stake {
				pool_address: pool_address.to_vec(),
				amount: amount.to_string(),
			})),
			nonce,
			fee_limit,
			signature,
			verifying_key,
			eth_original_transaction,
		}),
		system::transaction::TransactionType::UnStake { pool_address, amount } => Ok(TransactionV2 {
			tx_type: rpc_model::TransactionType::Unstake as i32,
			transaction: Some(transaction_v2::Transaction::Unstake(UnStake {
				pool_address: pool_address.to_vec(),
				amount: amount.to_string(),
			})),
			nonce,
			fee_limit,
			signature,
			verifying_key,
			eth_original_transaction,
		}),
		system::transaction::TransactionType::CreateStakingPool { .. }
		| system::transaction::TransactionType::StakingPoolContract { .. } => {
			Err(NodeError::InvalidTransactionType(format!(
				"Invalid transaction type: {:?}",
				txn.transaction_type
			)))
		},
	}
}

/// Convert from a system transaction type to a proto transaction_v2 type
pub fn to_proto_transaction_v3(
	txn: system::transaction::Transaction,
) -> Result<TransactionV3, NodeError> {
	let nonce = txn.nonce.to_string();
	let fee_limit = txn.fee_limit.to_string();
	let signature = txn.signature;
	let verifying_key = txn.verifying_key;
	let eth_original_transaction = txn.eth_original_transaction;
	
	let transaction_version = match txn.version {
		system::transaction::TransactionVersion::V1 => rpc_model::TransactionVersion::V1,
		system::transaction::TransactionVersion::V2 => rpc_model::TransactionVersion::V2,
		system::transaction::TransactionVersion::V3 => rpc_model::TransactionVersion::V3,
	};

	match txn.transaction_type {
		system::transaction::TransactionType::NativeTokenTransfer(address, balance) =>
			Ok(TransactionV3 {
				version: transaction_version.into(),
				tx_type: rpc_model::TransactionType::NativeTokenTransfer as i32,
				transaction: Some(transaction_v3::Transaction::NativeTokenTransfer(
					NativeTokenTransfer { address: address.to_vec(), amount: balance.to_string() },
				)),
				nonce,
				fee_limit,
				signature,
				verifying_key,
				eth_original_transaction,
			}),
		system::transaction::TransactionType::SmartContractDeployment {
			access_type,
			contract_type,
			contract_code,
			deposit,
			salt,
		} => Ok(TransactionV3 {
			version: transaction_version.into(),
			tx_type: rpc_model::TransactionType::SmartContractDeployment as i32,
			transaction: Some(transaction_v3::Transaction::SmartContractDeployment(
				SmartContractDeploymentV2 {
					access_type: access_type as i32,
					contract_type: contract_type as i32,
					contract_code,
					deposit: deposit.to_string(),
					salt,
				},
			)),
			nonce,
			fee_limit,
			signature,
			verifying_key,
			eth_original_transaction,
		}),
		system::transaction::TransactionType::SmartContractInit {
			contract_code_address,
			arguments, 
			deposit
		} =>
			Ok(TransactionV3 {
				version: transaction_version.into(),
				tx_type: rpc_model::TransactionType::SmartContractInstantiation as i32,
				transaction: Some(transaction_v3::Transaction::SmartContractInit(SmartContractInitV2 {
					contract_code_address: contract_code_address.to_vec(),
					arguments,
					deposit: deposit.to_string(),
				})),
				nonce,
				fee_limit,
				signature,
				verifying_key,
				eth_original_transaction,
			}),
		system::transaction::TransactionType::SmartContractFunctionCall {
			contract_instance_address,
			function,
			arguments,
			deposit,
		} => Ok(TransactionV3 {
			version: transaction_version.into(),
			tx_type: rpc_model::TransactionType::SmartContractFunctionCall as i32,
			transaction: Some(transaction_v3::Transaction::SmartContractFunctionCall(
				SmartContractFunctionCallV2 {
					contract_instance_address: contract_instance_address.to_vec(),
					function_name: function,
					arguments,
					deposit: deposit.to_string(),
				},
			)),
			nonce,
			fee_limit,
			signature,
			verifying_key,
			eth_original_transaction,
		}),
		system::transaction::TransactionType::Stake { pool_address, amount } => Ok(TransactionV3 {
			version: transaction_version.into(),
			tx_type: rpc_model::TransactionType::Stake as i32,
			transaction: Some(transaction_v3::Transaction::Stake(Stake {
				pool_address: pool_address.to_vec(),
				amount: amount.to_string(),
			})),
			nonce,
			fee_limit,
			signature,
			verifying_key,
			eth_original_transaction,
		}),
		system::transaction::TransactionType::UnStake { pool_address, amount } => Ok(TransactionV3 {
			version: transaction_version.into(),
			tx_type: rpc_model::TransactionType::Unstake as i32,
			transaction: Some(transaction_v3::Transaction::Unstake(UnStake {
				pool_address: pool_address.to_vec(),
				amount: amount.to_string(),
			})),
			nonce,
			fee_limit,
			signature,
			verifying_key,
			eth_original_transaction,
		}),
		system::transaction::TransactionType::CreateStakingPool { .. }
		| system::transaction::TransactionType::StakingPoolContract { .. } => {
			Err(NodeError::InvalidTransactionType(format!(
				"Invalid transaction type: {:?}",
				txn.transaction_type
			)))
		},
	}
}

/// Convert a rpc transaction type to a system transaction type
pub async fn from_rpc_transaction(
	txn: l1x_rpc::transaction::Transaction,
) -> Result<system::transaction::Transaction, Error> {
	let nonce = txn.nonce;
	let transaction_type: system::transaction::TransactionType = match txn.transaction_type {
		l1x_rpc::transaction::TransactionType::NativeTokenTransfer(address, balance) =>
			system::transaction::TransactionType::NativeTokenTransfer(address, balance),
		l1x_rpc::transaction::TransactionType::SmartContractDeployment {
			access_type,
			contract_type,
			contract_code,
			value,
			salt,
		} => system::transaction::TransactionType::SmartContractDeployment {
			access_type: (access_type as i8).try_into()?,
			contract_type: (contract_type as i8).try_into()?,
			contract_code,
			deposit: value,
			salt,
		},
		l1x_rpc::transaction::TransactionType::SmartContractInit(address, arguments) =>
			system::transaction::TransactionType::SmartContractInit {
				contract_code_address: address,
				arguments,
				deposit: 0,
			},
		l1x_rpc::transaction::TransactionType::SmartContractFunctionCall {
			contract_instance_address,
			function,
			arguments,
		} => system::transaction::TransactionType::SmartContractFunctionCall {
			contract_instance_address,
			function,
			arguments,
			deposit: 0,
		},
		l1x_rpc::transaction::TransactionType::CreateStakingPool {
			contract_instance_address,
			min_stake,
			max_stake,
			min_pool_balance,
			max_pool_balance,
			staking_period,
		} => system::transaction::TransactionType::CreateStakingPool {
			contract_instance_address,
			min_stake,
			max_stake,
			min_pool_balance,
			max_pool_balance,
			staking_period,
		},
		l1x_rpc::transaction::TransactionType::Stake { pool_address, amount } =>
			system::transaction::TransactionType::Stake { pool_address, amount },
		l1x_rpc::transaction::TransactionType::UnStake { pool_address, amount } =>
			system::transaction::TransactionType::UnStake { pool_address, amount },
		l1x_rpc::transaction::TransactionType::StakingPoolContract {
			pool_address,
			contract_instance_address,
		} => system::transaction::TransactionType::StakingPoolContract {
			pool_address,
			contract_instance_address,
		},
	};
	let fee_limit = txn.fee_limit;
	let signature = txn.signature;
	let verifying_key = txn.verifying_key;

	let sys_txn = system::transaction::Transaction {
		version: system::transaction::TransactionVersion::V1,
		nonce,
		transaction_type,
		fee_limit,
		signature,
		verifying_key,
		eth_original_transaction: None,
	};

	Ok(sys_txn)
}

/// Convert a proto transaction type to a system transaction type
pub async fn from_proto_transaction(
	txn: l1x_rpc::rpc_model::Transaction,
) -> Result<system::transaction::Transaction, Error> {
	let transaction_type =
		txn.transaction.ok_or(anyhow!("Missing transaction in proto transaction"))?;

	let transaction_type = match transaction_type {
		transaction::Transaction::NativeTokenTransfer(native_token_transfer) => {
			let address = bytes_to_address(native_token_transfer.address)?;
			let balance = native_token_transfer.amount.parse::<Balance>()?;
			system::transaction::TransactionType::NativeTokenTransfer(address, balance)
		},
		transaction::Transaction::SmartContractDeployment(smart_contract_deployment) => {
			let access_type = smart_contract_deployment.access_type;
			let contract_type = smart_contract_deployment.contract_type;
			let contract_code = smart_contract_deployment.contract_code;
			let value: Balance = smart_contract_deployment.value.into();
			let salt = smart_contract_deployment.salt;
			system::transaction::TransactionType::SmartContractDeployment {
				access_type: (access_type as i8).try_into()?,
				contract_type: (contract_type as i8).try_into()?,
				contract_code,
				deposit: value,
				salt,
			}
		},
		transaction::Transaction::SmartContractInit(smart_contract_init) => {
			let address = bytes_to_address(smart_contract_init.address)?;
			let arguments = smart_contract_init.arguments;
			system::transaction::TransactionType::SmartContractInit {
				contract_code_address: address,
				arguments,
				deposit: 0
			}
		},
		transaction::Transaction::SmartContractFunctionCall(smart_contract_function_call) => {
			let contract_instance_address =
				bytes_to_address(smart_contract_function_call.contract_address)?;
			let function = smart_contract_function_call.function_name;
			let arguments = smart_contract_function_call.arguments;
			system::transaction::TransactionType::SmartContractFunctionCall {
				contract_instance_address,
				function,
				arguments,
				deposit: 0,
			}
		},
		transaction::Transaction::Stake(stake) => {
			let pool_address = bytes_to_address(stake.pool_address)?;
			let amount = stake.amount.parse::<Balance>()?;
			system::transaction::TransactionType::Stake { pool_address, amount }
		},
		transaction::Transaction::Unstake(unstake) => {
			let pool_address = bytes_to_address(unstake.pool_address)?;
			let amount = unstake.amount.parse::<Balance>()?;
			system::transaction::TransactionType::UnStake { pool_address, amount }
		},
	};

	let nonce = txn.nonce.parse::<u128>()?;
	let fee_limit = txn.fee_limit.parse::<Balance>()?;
	let signature = txn.signature;
	let verifying_key = txn.verifying_key;

	let sys_txn = system::transaction::Transaction {
		version: system::transaction::TransactionVersion::V1,
		nonce,
		transaction_type,
		fee_limit,
		signature,
		verifying_key,
		eth_original_transaction: None,
	};

	Ok(sys_txn)
}

/// Convert a proto transaction_v2 type to a system transaction type
pub async fn from_proto_transaction_v2(
	txn: l1x_rpc::rpc_model::TransactionV2,
) -> Result<system::transaction::Transaction, Error> {
	let transaction_type =
		txn.transaction.ok_or(anyhow!("Missing transaction in proto transaction"))?;

	let transaction_type = match transaction_type {
		transaction_v2::Transaction::NativeTokenTransfer(native_token_transfer) => {
			let address = bytes_to_address(native_token_transfer.address)?;
			let balance = native_token_transfer.amount.parse::<Balance>()?;
			system::transaction::TransactionType::NativeTokenTransfer(address, balance)
		},
		transaction_v2::Transaction::SmartContractDeployment(smart_contract_deployment) => {
			let access_type = smart_contract_deployment.access_type;
			let contract_type = smart_contract_deployment.contract_type;
			let contract_code = smart_contract_deployment.contract_code;
			let value: Balance = smart_contract_deployment.value.into();
			let salt = smart_contract_deployment.salt;
			system::transaction::TransactionType::SmartContractDeployment {
				access_type: (access_type as i8).try_into()?,
				contract_type: (contract_type as i8).try_into()?,
				contract_code,
				deposit: value,
				salt,
			}
		},
		transaction_v2::Transaction::SmartContractInit(smart_contract_init) => {
			let address = bytes_to_address(smart_contract_init.address)?;
			let arguments = smart_contract_init.arguments;
			system::transaction::TransactionType::SmartContractInit {
				contract_code_address: address,
				arguments,
				deposit: 0,
			}
		},
		transaction_v2::Transaction::SmartContractFunctionCall(smart_contract_function_call) => {
			let contract_instance_address =
				bytes_to_address(smart_contract_function_call.contract_address)?;
			let function = smart_contract_function_call.function_name;
			let arguments = smart_contract_function_call.arguments;
			system::transaction::TransactionType::SmartContractFunctionCall {
				contract_instance_address,
				function,
				arguments,
				deposit: 0,
			}
		},
		transaction_v2::Transaction::Stake(stake) => {
			let pool_address = bytes_to_address(stake.pool_address)?;
			let amount = stake.amount.parse::<Balance>()?;
			system::transaction::TransactionType::Stake { pool_address, amount }
		},
		transaction_v2::Transaction::Unstake(unstake) => {
			let pool_address = bytes_to_address(unstake.pool_address)?;
			let amount = unstake.amount.parse::<Balance>()?;
			system::transaction::TransactionType::UnStake { pool_address, amount }
		},
	};

	let nonce = txn.nonce.parse::<u128>()?;
	let fee_limit = txn.fee_limit.parse::<Balance>()?;
	let signature = txn.signature;
	let verifying_key = txn.verifying_key;
	let eth_original_transaction = txn.eth_original_transaction;

	let sys_txn = system::transaction::Transaction {
		version: system::transaction::TransactionVersion::V2,
		nonce,
		transaction_type,
		fee_limit,
		signature,
		verifying_key,
		eth_original_transaction,
	};

	Ok(sys_txn)
}

/// Convert a proto transaction_v2 type to a system transaction type
pub async fn from_proto_transaction_v3(
	txn: l1x_rpc::rpc_model::TransactionV3,
) -> Result<system::transaction::Transaction, Error> {
	let transaction_type =
		txn.transaction.ok_or(anyhow!("Missing transaction in proto transaction"))?;

	let transaction_version = rpc_model::TransactionVersion::from_i32(txn.version)
		.ok_or(anyhow!("Unknown trasaction v3 version: {}", txn.version))?;

	let transaction_version = match transaction_version {
		rpc_model::TransactionVersion::V1 => system::transaction::TransactionVersion::V1,
		rpc_model::TransactionVersion::V2 => system::transaction::TransactionVersion::V2,
		rpc_model::TransactionVersion::V3 => system::transaction::TransactionVersion::V3,
	};
	

	let transaction_type = match transaction_type {
		transaction_v3::Transaction::NativeTokenTransfer(native_token_transfer) => {
			let address = bytes_to_address(native_token_transfer.address)?;
			let balance = native_token_transfer.amount.parse::<Balance>()?;
			system::transaction::TransactionType::NativeTokenTransfer(address, balance)
		},
		transaction_v3::Transaction::SmartContractDeployment(smart_contract_deployment) => {
			let access_type = smart_contract_deployment.access_type;
			let contract_type = smart_contract_deployment.contract_type;
			let contract_code = smart_contract_deployment.contract_code;
			let deposit  = smart_contract_deployment.deposit.parse::<Balance>()?;
			let salt = smart_contract_deployment.salt;
			system::transaction::TransactionType::SmartContractDeployment {
				access_type: (access_type as i8).try_into()?,
				contract_type: (contract_type as i8).try_into()?,
				contract_code,
				deposit,
				salt,
			}
		},
		transaction_v3::Transaction::SmartContractInit(smart_contract_init) => {
			let address = bytes_to_address(smart_contract_init.contract_code_address)?;
			let arguments = smart_contract_init.arguments;
			let deposit  = smart_contract_init.deposit.parse::<Balance>()?;
			system::transaction::TransactionType::SmartContractInit {
				contract_code_address: address,
				arguments,
				deposit,
			}
		},
		transaction_v3::Transaction::SmartContractFunctionCall(smart_contract_function_call) => {
			let contract_instance_address =
				bytes_to_address(smart_contract_function_call.contract_instance_address)?;
			let function = smart_contract_function_call.function_name;
			let arguments = smart_contract_function_call.arguments;
			let deposit  = smart_contract_function_call.deposit.parse::<Balance>()?;
			system::transaction::TransactionType::SmartContractFunctionCall {
				contract_instance_address,
				function,
				arguments,
				deposit,
			}
		},
		transaction_v3::Transaction::Stake(stake) => {
			let pool_address = bytes_to_address(stake.pool_address)?;
			let amount = stake.amount.parse::<Balance>()?;
			system::transaction::TransactionType::Stake { pool_address, amount }
		},
		transaction_v3::Transaction::Unstake(unstake) => {
			let pool_address = bytes_to_address(unstake.pool_address)?;
			let amount = unstake.amount.parse::<Balance>()?;
			system::transaction::TransactionType::UnStake { pool_address, amount }
		},
	};

	let nonce = txn.nonce.parse::<u128>()?;
	let fee_limit = txn.fee_limit.parse::<Balance>()?;
	let signature = txn.signature;
	let verifying_key = txn.verifying_key;
	let eth_original_transaction = txn.eth_original_transaction;

	let sys_txn = system::transaction::Transaction {
		version: transaction_version,
		nonce,
		transaction_type,
		fee_limit,
		signature,
		verifying_key,
		eth_original_transaction,
	};

	Ok(sys_txn)
}

/// Convert a proto block type to a system block type
pub async fn from_proto_block(
	block: l1x_rpc::rpc_model::Block,
) -> Result<system::block::Block, Error> {
	let block_number = block.number.parse::<u128>()?;
	let block_hash = bytes_to_hash(hex::decode(block.hash)?)?;
	let parent_hash = bytes_to_hash(hex::decode(block.parent_hash)?)?;
	let block_type = match block.block_type {
		0 => BlockType::L1XTokenBlock,
		1 => BlockType::L1XTokenBlock,
		2 => BlockType::L1XContractBlock,
		3 => BlockType::XTalkTokenBlock, // Possibly a place for state issues/differences
		_ => return Err(anyhow!("Invalid block type: {}", block.block_type)),
	};
	let cluster_address = bytes_to_address(hex::decode(block.cluster_address)?)?;
	let num_transactions = block.transactions.len() as i32;

	let sys_block_header = system::block_header::BlockHeader {
		block_number,
		block_hash,
		parent_hash,
		block_type,
		cluster_address,
		timestamp: block.timestamp,
		num_transactions,
		block_version: 1,
		state_hash: [0; 32],
		epoch: 0,
	};

	let mut transactions: Vec<system::transaction::Transaction> = Vec::new();
	for txn in block.transactions {
		let nested_txn = txn.transaction.ok_or(anyhow!("Missing transaction in proto block"))?;

		// Convert to system type transaction
		let converted_txn = from_proto_transaction(nested_txn).await?;
		transactions.push(converted_txn);
	}

	// Form the system type block
	let sys_block = system::block::Block { block_header: sys_block_header, transactions };

	Ok(sys_block)
}

/// Convert a proto block(with transaction_v2) type to a system block type
pub async fn from_proto_block_v2(
	block: l1x_rpc::rpc_model::BlockV2,
) -> Result<system::block::Block, Error> {
	let block_number = block.number.parse::<u128>()?;
	let block_hash = bytes_to_hash(hex::decode(block.hash)?)?;
	let parent_hash = bytes_to_hash(hex::decode(block.parent_hash)?)?;
	let block_type = match block.block_type {
		0 => BlockType::L1XTokenBlock,
		1 => BlockType::L1XTokenBlock,
		2 => BlockType::L1XContractBlock,
		3 => BlockType::XTalkTokenBlock, // Possibly a place for state issues/differences
		_ => return Err(anyhow!("Invalid block type: {}", block.block_type)),
	};
	let cluster_address = bytes_to_address(hex::decode(block.cluster_address)?)?;
	let num_transactions = block.transactions.len() as i32;

	let sys_block_header = system::block_header::BlockHeader{
		block_number,
		block_hash,
		parent_hash,
		block_type,
		cluster_address,
		timestamp: block.timestamp,
		num_transactions,
		block_version: 2,
		state_hash: [0; 32],
		epoch: 0,
	};

	let mut transactions: Vec<system::transaction::Transaction> = Vec::new();
	for txn in block.transactions {
		let nested_txn = txn.transaction.ok_or(anyhow!("Missing transaction in proto block"))?;

		// Convert to system type transaction
		let converted_txn = from_proto_transaction_v2(nested_txn).await?;
		transactions.push(converted_txn);
	}

	// Form the system type block
	let sys_block = system::block::Block{ block_header: sys_block_header, transactions };

	Ok(sys_block)
}

/// Convert a proto block(with transaction_v3) type to a system block type
pub async fn from_proto_block_v3(
	block: l1x_rpc::rpc_model::BlockV3,
) -> Result<system::block::Block, Error> {
	let block_number = block.number.parse::<u128>()?;
	let block_hash = bytes_to_hash(hex::decode(block.hash)?)?;
	let parent_hash = bytes_to_hash(hex::decode(block.parent_hash)?)?;
	let state_hash = bytes_to_hash(hex::decode(block.state_hash)?)?;
	let block_version = block.block_version.parse::<u32>()?;
	let epoch = block.epoch.parse::<u64>()?;
	let block_type = block.block_type.try_into()?;
	let cluster_address = bytes_to_address(hex::decode(block.cluster_address)?)?;
	let num_transactions = block.transactions.len() as i32;

	let sys_block_header = system::block_header::BlockHeader {
		block_number,
		block_hash,
		parent_hash,
		block_type,
		cluster_address,
		timestamp: block.timestamp,
		num_transactions,
		block_version,
		state_hash,
		epoch,
	};

	let mut transactions: Vec<system::transaction::Transaction> = Vec::new();
	for txn in block.transactions {
		let nested_txn = txn.transaction.ok_or(anyhow!("Missing transaction in proto block"))?;

		// Convert to system type transaction
		let converted_txn = from_proto_transaction_v3(nested_txn).await?;
		transactions.push(converted_txn);
	}

	// Form the system type block
	let sys_block = system::block::Block { block_header: sys_block_header, transactions };

	Ok(sys_block)
}

pub async fn from_proto_vote_result(
	block: system::block::Block,
	vote_result: VoteResultShort
) -> Result<VoteResult, Error> {
	let vote_signed_payload = VoteResultSignPayload {
		block_number: block.block_header.block_number,
		block_hash: block.block_header.block_hash,
		cluster_address: block.block_header.cluster_address,
		vote_passed: vote_result.vote_passed,
		votes: bincode::deserialize(&vote_result.votes).map_err(|e| anyhow!("Unable to deserialize votes bytes: {:?}", e))?,
	};

	Ok(VoteResult {
		data: vote_signed_payload,
		validator_address: vote_result
			.validator_address
			.try_into()
			.map_err(|_| anyhow!("Unable to convert validator_address from vote_result's data"))?,
		signature: vote_result.signature,
		verifying_key: vote_result.verifying_key,
	})
}

pub fn convert_to_big_decimal(bal: primitives::Balance) -> BigDecimal {
	let int_val = BigInt::from(bal);
	BigDecimal::new(int_val, 0)
}

pub fn convert_to_big_decimal_balance(bal: primitives::Balance) -> BigDecimal {
	let int_val = BigInt::from(bal);
	BigDecimal::new(int_val, 0)
}

pub fn convert_to_big_decimal_block_number(bal: primitives::BlockNumber) -> BigDecimal {
	let int_val = BigInt::from(bal);
	BigDecimal::new(int_val, 0)
}

pub fn convert_to_big_decimal_epoch(epoch: primitives::Epoch) -> BigDecimal {
	let int_val = BigInt::from(epoch);
	BigDecimal::new(int_val, 0)
}

pub fn convert_to_big_decimal_tx_sequence(bal: primitives::TransactionSequence) -> BigDecimal {
	let int_val = BigInt::from(bal);
	BigDecimal::new(int_val, 0)
}

pub fn convert_f64_to_big_decimal(bal: f64) -> BigDecimal {
	BigDecimal::from_f64(bal).unwrap_or_else(|| BigDecimal::from(0))
}

pub fn to_rpc_node_info(system_node_info: &SystemNodeInfo) -> Result<RpcNodeInfo, Error> {
    Ok(RpcNodeInfo {
        address: system_node_info.address.to_vec(),
        peer_id: system_node_info.peer_id.clone(),
        joined_epoch: system_node_info.joined_epoch,
        ip_address: system_node_info.data.ip_address.clone(),
        metadata: system_node_info.data.metadata.clone(),
        cluster_address: system_node_info.data.cluster_address.to_vec(),
        signature: system_node_info.signature.clone(),
        verifying_key: system_node_info.verifying_key.clone(),
    })
}

pub fn to_rpc_node_health(system_node_health: &SystemNodeHealth) -> Result<RpcNodeHealth, Error> {
	Ok(RpcNodeHealth {
		measured_peer_id: system_node_health.measured_peer_id.clone(),
		peer_id: system_node_health.peer_id.clone(),
		epoch: system_node_health.epoch,
		joined_epoch: system_node_health.joined_epoch,
		uptime_percentage: system_node_health.uptime_percentage,
		response_time_ms: system_node_health.response_time_ms,
		transaction_count: system_node_health.transaction_count,
		block_proposal_count: system_node_health.block_proposal_count,
		anomaly_score: system_node_health.anomaly_score,
		node_health_version: system_node_health.node_health_version,
	})
}

pub fn to_rpc_validator(system_validator: &SystemValidator) -> Result<RpcValidator, Error> {
	Ok(RpcValidator {
		address: system_validator.address.to_vec(),
		cluster_address: system_validator.cluster_address.to_vec(),
		epoch: system_validator.epoch,
		stake: system_validator.stake.to_string(),
		xscore: system_validator.xscore
	})
}

pub fn to_rpc_block_header_v3(block_header: BlockHeader) -> BlockHeaderV3 {
	BlockHeaderV3 {
		block_number: block_header.block_number as u64,
		block_hash: hex::encode(block_header.block_hash),
		parent_hash: hex::encode(block_header.parent_hash),
		timestamp: block_header.timestamp,
		block_type: block_header.block_type as i32,
		cluster_address: hex::encode(block_header.cluster_address),
		num_transactions: block_header.num_transactions as u32,
		state_hash: hex::encode(block_header.state_hash),
		block_version: block_header.block_version.to_string(),
		epoch: block_header.epoch.to_string(),
	}
}

pub fn to_rpc_runtime_staking_info(runtime_staking_info_cache: Arc<RuntimeStakingInfoCache>) -> RuntimeStakingInfo {
	RuntimeStakingInfo {
		nodes: runtime_staking_info_cache
			.nodes
			.iter()
			.map(|(address, info)| (hex::encode(address), info.into()))
			.collect(),
		reward_node_map: runtime_staking_info_cache
			.reward_node_map
			.iter()
			.map(|(wallet_address, node_address)| (hex::encode(*wallet_address), hex::encode(*node_address)))
			.collect(),
	}
}

pub fn to_rpc_runtime_config(runtime_config_cache: Arc<RuntimeConfigCache>) -> RuntimeConfig {
	RuntimeConfig {
		org_nodes: runtime_config_cache.org_nodes.iter().map(|address| hex::encode(&address)).collect(),
		whitelisted_nodes: runtime_config_cache
			.whitelisted_nodes
			.as_ref()
			.map(|nodes| nodes.iter().map(|address| hex::encode(address)).collect::<Vec<_>>())
			.unwrap_or_else(Vec::new),
		whitelisted_block_proposers: runtime_config_cache
			.whitelisted_block_proposers
			.as_ref()
			.map(|nodes| nodes.iter().map(|address| hex::encode(address)).collect::<Vec<_>>())
			.unwrap_or_else(Vec::new),
		blacklisted_nodes: runtime_config_cache
			.blacklisted_nodes
			.as_ref()
			.map(|nodes| nodes.iter().map(|address| hex::encode(address)).collect::<Vec<_>>())
			.unwrap_or_else(Vec::new),
		blacklisted_block_proposers: runtime_config_cache
			.blacklisted_block_proposers
			.as_ref()
			.map(|nodes| nodes.iter().map(|address| hex::encode(address)).collect::<Vec<_>>())
			.unwrap_or_else(Vec::new),
		max_block_height: runtime_config_cache.max_block_height,
		max_validators: runtime_config_cache.max_validators,
		xscore: Some(to_rpc_xscore_config(&runtime_config_cache.xscore)),
		stake_score: Some(to_rpc_stake_score_config(&runtime_config_cache.stake_score)),
		kin_score: Some(to_rpc_kin_score_config(&runtime_config_cache.kin_score)),
		rewards: Some(to_rpc_validator_rewards_config(&runtime_config_cache.rewards)),
	}
}

pub fn to_rpc_runtime_deny_config(runtime_deny_config_cache: Arc<RuntimeDenyConfigCache>) -> RuntimeDenyConfig {
	RuntimeDenyConfig {
		whitelisted_addresses: Some(to_rpc_wb_addresses(runtime_deny_config_cache.whitelisted_addresses.as_ref())),
		blacklisted_addresses: Some(to_rpc_wb_addresses(runtime_deny_config_cache.blacklisted_addresses.as_ref())),
	}
}

fn to_rpc_wb_addresses(wb_addresses: Option<&WBAddresses>) -> WbAddresses {
	WbAddresses {
		sender_addresses: wb_addresses
			.map(|addresses| addresses.sender_addresses.iter().map(|address| hex::encode(address)).collect::<Vec<_>>())
			.unwrap_or_default(),
		receiver_addresses: wb_addresses
			.map(|addresses| addresses.receiver_addresses.iter().map(|address| hex::encode(address)).collect::<Vec<_>>())
			.unwrap_or_default(),
	}
}



fn to_rpc_xscore_config(xscore_config: &XscoreConfig) -> RpcXscoreConfig{
	RpcXscoreConfig{
		weight_for_kinscore: xscore_config.weight_for_kinscore,
		weight_for_stakescore: xscore_config.weight_for_stakescore,
		xscore_threshold: xscore_config.xscore_threshold,
	}
}

fn to_rpc_stake_score_config(stake_score_config: &StakeScoreConfig) -> RpcStakeScoreConfig{
	RpcStakeScoreConfig{
		min_balance: (stake_score_config.min_balance / 10 ^ 18).to_u64().unwrap_or(u64::MAX),
		max_balance: (stake_score_config.max_balance / 10 ^ 18).to_u64().unwrap_or(u64::MAX),
		min_stake_age: stake_score_config.min_stake_age,
		max_stake_age: stake_score_config.max_stake_age,
		min_lock_period: stake_score_config.min_lock_period,
		max_lock_period: stake_score_config.max_lock_period,
		weight_for_stake_balance_6: stake_score_config.weight_for_stake_balance_6,
		weight_for_stake_age_6: stake_score_config.weight_for_stake_age_6,
		weight_for_locking_period_6: stake_score_config.weight_for_locking_period_6,
		weight_for_stake_balance_12: stake_score_config.weight_for_stake_balance_12,
		weight_for_stake_age_12: stake_score_config.weight_for_stake_age_12,
		weight_for_locking_period_12: stake_score_config.weight_for_locking_period_12,
		weight_for_stake_balance_18: stake_score_config.weight_for_stake_balance_18,
		weight_for_stake_age_18: stake_score_config.weight_for_stake_age_18,
		weight_for_locking_period_18: stake_score_config.weight_for_locking_period_18,
	}
}

fn to_rpc_kin_score_config(kin_score_config: &KinScoreConfig) -> RpcKinScoreConfig{
	RpcKinScoreConfig{
		min_uptime: kin_score_config.min_uptime,
		max_uptime: kin_score_config.max_uptime,
		min_participation: kin_score_config.min_participation,
		max_participation: kin_score_config.max_participation,
		min_response_time: kin_score_config.min_response_time,
		max_response_time: kin_score_config.max_response_time,
		min_security_measure: kin_score_config.min_security_measure,
		max_security_measure: kin_score_config.max_security_measure,
		xscore_threshold: kin_score_config.xscore_threshold,
		weight_for_uptime: kin_score_config.weight_for_uptime,
		weight_for_participation_history: kin_score_config.weight_for_participation_history,
		weight_for_response_time: kin_score_config.weight_for_response_time,
		weight_for_security_measure: kin_score_config.weight_for_security_measure,
	}
}

fn to_rpc_validator_rewards_config(vaidator_rewards: &ValidatorRewards) -> RpcValidatorRewards{
	RpcValidatorRewards{
		validated_block_reward: vaidator_rewards.validated_block_reward.0.to_string(),
	}
}
