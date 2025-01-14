use crate::{
	consts::READONLY_CALL_DEFAULT_GAS_LIMIT, execute_common::ContractExecutor, execute_contract_evm::ExecuteContract as EVMExecutor, execute_contract_l1xvm::ExecuteContract as L1XVMExecutor, execute_contract_xtalk::ExecuteContract as XTalkExecutor, execute_fee, execute_staking::ExecuteStaking, execute_token::ExecuteToken
};
use anyhow::anyhow;
use evm_runtime::{Capture, Context, CreateScheme, ExitError, ExitReason};
use l1x_evm::tagged_runtime::create_evm_address;
use log::info;
use primitive_types::{H160, H256};
use primitives::*;
use serde_json::json;
use sha3::{Digest, Keccak256};
use std::{borrow::Cow, sync::Arc};
use system::{
	contract::ContractType, network::EventBroadcast, transaction::{Transaction, TransactionType}
};
use state::UpdatedState;
use tokio::sync::broadcast;
use fee::fee_manager::FeeManager;

pub struct ExecuteTransaction {}

impl ExecuteTransaction {
	pub fn execute_transaction<'a>(
		transaction: &Transaction,
		account_address: &Address,
		cluster_address: &Address,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> (anyhow::Result<()>, Option<EventData>, Balance, Gas) {
		let mut fee_manager = match FeeManager::new(transaction.fee_limit) {
			Ok(manager) => manager,
			Err(e) => return (Err(e), None, 0, 0),
		};

		// Create execution fee instance. Fee can be charged within transaction.fee_limit
		let mut execution_fee = execute_fee::ExecuteFee::new(transaction, execute_fee::SystemFeeConfig::new());

		// Get transaction size fee
		match execution_fee.get_transaction_size_fee() {
			Ok(fee) => {
				// Update total_fee_used
				if let Err(e) = fee_manager.update_fee(fee) {
					return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
				}
			},
			Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
		}

		let cloned_transaction = transaction.clone();
		let transaction_hash = match transaction.transaction_hash() {
			Ok(v) => v,
			Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
		};
		let account_nonce = match updated_state.get_nonce(&account_address) {
			Ok(nonce) => nonce,
			Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
		};
		let (result, event) = match cloned_transaction.transaction_type {
			TransactionType::NativeTokenTransfer(recipient, amount) => {
				// Update total_fee_used
				if let Err(e) = fee_manager.update_fee(execution_fee.get_token_transfer_fee()) {
					return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
				}

				info!("Executing NativeTokenTransfer");
				match ExecuteToken::execute_native_token_transfer(
					account_address,
					&recipient,
					amount,
					updated_state,
				) {
					Ok(_contract_instance_address) => {
						let val = match serde_json::to_vec(&json!({
							"event_type": "native_token_transferred",
							"from" : account_address,
							"to" : &recipient,
							"amount" : amount.to_string(),
						})) {
							Ok(v) => v,
							Err(e) => return (Err(anyhow!("{:?}", e)), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
						};
						(Ok(()), Some(val))
					},
					Err(e) => {
						let msg = format!("Error executing NativeTokenTransfer - {}", e);
						(Err(anyhow!(msg)), None)
					},
				}
			},
			TransactionType::SmartContractDeployment {
				access_type,
				contract_type,
				contract_code,
				deposit,
				salt,
			} => {
				let salt = match TryInto::<[u8; 32]>::try_into(salt) {
					Ok(salt) => H256::from_slice(&salt),
					Err(_) => return (Err(anyhow!("Incorrect salt")), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
				};

				let executor: Box<dyn ContractExecutor + Send + Sync> = match contract_type {
					ContractType::L1XVM => Box::new(L1XVMExecutor),
					ContractType::EVM => Box::new(EVMExecutor),
					ContractType::XTALK => Box::new(XTalkExecutor),
				};
				let caller = account_address;
				let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
				let scheme = if contract_type == ContractType::EVM {
					let schm = CreateScheme::Legacy { caller: caller.into() };
					schm
				} else {
					let schm = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
					schm
				};
				let contract_instance_address = create_evm_address(scheme, account_nonce);
				let context = Context {
					// Execution address.
					address: contract_instance_address, // Does not affect pool id
					// Caller of the EVM.
					caller: H160(*account_address), // Does not affect pool id
					// Apparent value of the EVM.
					apparent_value: deposit.into(),
				};
				match execution_fee.get_contract_fee(&contract_type) {
					Ok(fee) => {
						// Update total_fee_used
						if let Err(e) = fee_manager.update_fee(fee) {
							return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
						}
					},
					Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
				}

				let gas_limit = match fee_manager.get_gas_limit() {
					Ok(limit) => limit,
					Err(error) => return (Err(error), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
				};

				let current_call_depth = 0;
				match executor
					.execute_contract_deployment(
						account_address,
						scheme,
						context,
						cluster_address,
						access_type as i8,
						&contract_code,
						account_nonce,
						gas_limit,
						deposit,
						&transaction_hash,
						block_number,
						block_hash,
						block_timestamp,
						current_call_depth,
						updated_state,
						event_tx,
					)
				{
					Ok((contract_address, burnt_gas)) => {
						// update burnt_gas and fee_used
						if let Err(e) = fee_manager.update_gas_and_fee_used(burnt_gas) {
							return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
						}

						let val = match serde_json::to_vec(&json!({
							"event_type": "smart_contract_deployed",
							"contract_type": match contract_type {
								ContractType::L1XVM => "l1xvm",
								ContractType::EVM => "evm",
								ContractType::XTALK => "xtalk"},
							"contract_address" : contract_address,
						})) {
							Ok(v) => v,
							Err(e) => return (Err(anyhow!("{:?}", e)), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
						};
						(Ok(()), Some(val))
					},
					Err(e) =>
						(Err(anyhow!("Error executing SmartContractDeployment - {}", e)), None),
				}
			},
			TransactionType::SmartContractInit {
				contract_code_address,
				arguments,
				deposit
			} => {
				let contract = {
					match updated_state.get_contract(&contract_code_address) {
						Ok(v) => v,
						Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
					}
				};
				let contract_type = match contract.r#type.try_into() {
					Ok(v) => v,
					Err(e) => return (Err(anyhow!("{:?}", e)), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
				};
				let executor: Box<dyn ContractExecutor + Send + Sync> = match contract_type {
					ContractType::L1XVM => Box::new(L1XVMExecutor),
					ContractType::EVM => Box::new(EVMExecutor),
					ContractType::XTALK => Box::new(XTalkExecutor),
				};
				let context = Context {
					// Execution address.
					address: H160(*account_address),
					// Caller of the EVM.
					caller: H160(*account_address), /* This is where the vault address is taken
					                                 * for pool id */
					// Apparent value of the EVM.
					apparent_value: deposit.into(),
				};

				// Charge contract init fee
				match execution_fee.get_contract_fee(&contract_type) {
					Ok(fee) => {
						// Update total_fee_used
						if let Err(e) = fee_manager.update_fee(fee) {
							return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
						}
					},
					Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
				}

				let gas_limit = match fee_manager.get_gas_limit() {
					Ok(limit) => limit,
					Err(error) => return (Err(error), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
				};

				let current_call_depth = 0;
				match executor
					.execute_contract_init(
						account_address,
						context,
						gas_limit,
						deposit,
						cluster_address,
						&contract,
						arguments,
						account_nonce,
						&transaction_hash,
						block_number,
						block_hash,
						block_timestamp,
						current_call_depth,
						updated_state,
						event_tx.clone(),
					)
				{
					Ok((contract_instance, burnt_gas)) => {
						// update burnt_gas and fee_used
						if let Err(e) = fee_manager.update_gas_and_fee_used(burnt_gas) {
							return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
						}

						let val = match serde_json::to_vec(&json!({
							"event_type": "smart_contract_initialized",
							"contract_instance_address" : contract_instance,
						})) {
							Ok(v) => v,
							Err(e) => return (Err(anyhow!("{:?}", e)), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
						};
						(Ok(()), Some(val))
					},
					Err(e) => (Err(anyhow!("Error executing SmartContractInit - {}", e)), None),
				}
			},
			TransactionType::SmartContractFunctionCall {
				contract_instance_address,
				function,
				arguments,
				deposit
			} => {
				let is_valid_contract_address = match updated_state.is_valid_contract_instance(&contract_instance_address) {
					Ok(valid) => valid,
					Err(e) => return (Err(e.into()), None, 0, 0),
				};

				if !is_valid_contract_address {
					// This is the case of native token transfer in evm

					// Update total_fee_used
					if let Err(e) = fee_manager.update_fee(execution_fee.get_token_transfer_fee()) {
						return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
					}
					info!("Executing NativeTokenTransfer");
					match ExecuteToken::execute_native_token_transfer(
						account_address,
						&contract_instance_address,
						deposit,
						updated_state,
					) {
						Ok(_) => {
							let val = match serde_json::to_vec(&json!({
							"event_type": "native_token_transferred",
							"from" : account_address,
							"to" : &contract_instance_address,
							"amount" : deposit.to_string(),
						})) {
								Ok(v) => v,
								Err(e) => return (Err(anyhow!("{:?}", e)), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
							};
							(Ok(()), Some(val))
						},
						Err(e) => {
							let msg = format!("Error executing NativeTokenTransfer - {}", e);
							(Err(anyhow!(msg)), None)
						},
					}
				} else {
					// function call execution
					let contract_instance = match updated_state.get_contract_instance(&contract_instance_address) {
						Ok(v) => v,
						Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
					};
					let contract = {
						match updated_state.get_contract(&contract_instance.contract_address) {
							Ok(v) => v,
							Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
						}
					};
					let contract_type = match contract.r#type.try_into() {
						Ok(v) => v,
						Err(e) => return (Err(anyhow!("{:?}", e)), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
					};
					let executor: Box<dyn ContractExecutor + Send + Sync> = match contract_type {
						ContractType::L1XVM => Box::new(L1XVMExecutor),
						ContractType::EVM => Box::new(EVMExecutor),
						ContractType::XTALK => Box::new(XTalkExecutor),
					};
					let context = Context {
						// Execution address.
						address: H160(contract_instance_address),
						// Caller of the EVM.
						caller: H160(*account_address), // This is where the vault address is taken for pool id
						// Apparent value of the EVM.
						apparent_value: deposit.into(),
					};

					// Charge contract call fee
					match execution_fee.get_contract_fee(&contract_type) {
						Ok(fee) => {
							// Update total_fee_used
							if let Err(e) = fee_manager.update_fee(fee) {
								return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used());
							}
						},
						Err(e) => return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
					}

					let gas_limit = match fee_manager.get_gas_limit() {
						Ok(limit) => limit,
						Err(error) => return (Err(error), None, fee_manager.get_fee_used(), fee_manager.get_gas_used()),
					};

					info!("Execute with Gas limit: {gas_limit}");
					let current_call_depth = 0;
					let (result, burnt_gas) = executor.execute_contract_function_call(
						account_address,
						context,
						gas_limit,
						deposit,
						&cluster_address,
						&contract,
						&contract_instance,
						&function,
						&arguments,
						account_nonce,
						&transaction_hash,
						false,
						block_number,
						block_hash,
						block_timestamp,
						current_call_depth,
						updated_state,
						event_tx,
					);
					let (reason, result) = match result {
						Capture::Exit(reason) => {
							let (reason, result) = reason;
							(reason, result)
						},
						Capture::Trap(_resolve) => (
							ExitReason::Error(ExitError::Other(Cow::Owned(
								"execute_transaction: Capture::Trap(resolve)".to_string(),
							))),
							Arc::new(vec![]),
						),
					};

					// update burnt_gas and fee_used
					if let Err(e) = fee_manager.update_gas_and_fee_used(burnt_gas) {
						return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used());
					}

					let (res, return_data) = Self::analyze_reason(&reason, (*result).clone());
					(res, return_data)
				}
			},
			TransactionType::CreateStakingPool {
				contract_instance_address,
				min_stake,
				max_stake,
				min_pool_balance,
				max_pool_balance,
				staking_period,
			} => {
				// Update total_fee_used
				if let Err(e) = fee_manager.update_fee(execution_fee.get_create_staking_pool_fee()) {
					return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
				}

				let execute_staking = ExecuteStaking {};
				match execute_staking
					.execute_native_staking_create_pool(
						account_address,
						cluster_address,
						account_nonce,
						block_number,
						contract_instance_address,
						min_stake,
						max_stake,
						min_pool_balance,
						max_pool_balance,
						staking_period,
						updated_state,
					)
				{
					Ok(address) => (Ok(()), Some(address.to_vec())),
					Err(e) => (Err(anyhow!("Error executing CreateStakingPool - {}", e)), None),
				}
			},
			TransactionType::Stake { pool_address, amount } => {
				// Update total_fee_used
				if let Err(e) = fee_manager.update_fee(execution_fee.get_stake_fee()) {
					return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
				}

				let execute_staking = ExecuteStaking {};
				match execute_staking
					.execute_native_staking_stake(
						&pool_address,
						account_address,
						block_number,
						amount,
						updated_state
					)
				{
					Ok(_) => (Ok(()), None),
					Err(e) => (Err(anyhow!("Error executing Stake - {}", e)), None),
				}
			},
			TransactionType::UnStake { pool_address, amount } => {
				// Update total_fee_used
				if let Err(e) = fee_manager.update_fee(execution_fee.get_unstake_fee()) {
					return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
				}

				let execute_staking = ExecuteStaking {};
				match execute_staking
					.execute_native_staking_un_stake(
						&pool_address,
						account_address,
						block_number,
						amount,
						updated_state
					)
				{
					Ok(_) => (Ok(()), None),
					Err(e) => (Err(anyhow!("Error executing UnStake - {}", e)), None),
				}
			},
			TransactionType::StakingPoolContract { pool_address, contract_instance_address } => {
				// Update total_fee_used
				if let Err(e) = fee_manager.update_fee(execution_fee.get_staking_pool_contract_fee()) {
					return (Err(e), None, fee_manager.get_fee_used(), fee_manager.get_gas_used())
				}

				let execute_staking = ExecuteStaking {};
				match execute_staking
					.execute_native_staking_update_contract(
						&pool_address,
						&contract_instance_address,
						account_address,
						block_number,
						updated_state
					)
				{
					Ok(_) => (Ok(()), None),
					Err(e) => (Err(anyhow!("Error executing StakingPoolContract - {}", e)), None),
				}
			},
		};
		(result, event, fee_manager.get_fee_used(), fee_manager.get_gas_used())
	}

	pub fn execute_estimate_fee<'a>(
		transaction: &Transaction,
		account_address: &Address,
		cluster_address: &Address,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> anyhow::Result<Balance, Balance> {
		// increment account's nonce
		let mut fee_manager = match FeeManager::new(transaction.fee_limit) {
			Ok(manager) => manager,
			Err(_) => return Err(0),
		};
		match updated_state.increment_nonce(&account_address) {
			Ok(_) => (),
			Err(_e) => return Err(fee_manager.get_fee_used()),
		}

		// Create execution fee instance. Fee can be charged within transaction.fee_limit
		let mut execution_fee = execute_fee::ExecuteFee::new(transaction, execute_fee::SystemFeeConfig::new());

		// Get transaction size fee
		if let Ok(fee) = execution_fee.get_transaction_size_fee() {
			if let Err(_e) = fee_manager.update_fee(fee) {
				return Err(fee_manager.get_fee_used())
			}
		} else {
			return Err(fee_manager.get_fee_used());
		}

		let cloned_transaction = transaction.clone();
		let transaction_hash = match transaction.transaction_hash() {
			Ok(v) => v,
			Err(_e) => return Err(fee_manager.get_fee_used()),
		};
		let account_nonce = match updated_state.get_nonce(&account_address) {
			Ok(nonce) => nonce,
			Err(_e) => return Err(fee_manager.get_fee_used()),
		};

		match cloned_transaction.transaction_type {
			TransactionType::NativeTokenTransfer(_recipient, _amount) => {
				if let Err(_e) = fee_manager.update_fee(execution_fee.get_token_transfer_fee()) {
					return Err(fee_manager.get_fee_used())
				}
				Ok(fee_manager.get_fee_used())
			},
			TransactionType::SmartContractDeployment {
				access_type,
				contract_type,
				contract_code,
				deposit,
				salt,
			} => {
				let salt = match TryInto::<[u8; 32]>::try_into(salt) {
					Ok(salt) => H256::from_slice(&salt),
					Err(_) => return Err(fee_manager.get_fee_used()),
				};

				let executor: Box<dyn ContractExecutor + Send + Sync> = match contract_type {
					ContractType::L1XVM => Box::new(L1XVMExecutor),
					ContractType::EVM => Box::new(EVMExecutor),
					ContractType::XTALK => Box::new(XTalkExecutor),
				};
				let caller = account_address;
				let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
				let scheme = if contract_type == ContractType::EVM {
					CreateScheme::Legacy { caller: caller.into() }
				} else {
					CreateScheme::Create2 { caller: caller.into(), code_hash, salt }
				};
				let contract_instance_address = create_evm_address(scheme, account_nonce);
				let context = Context {
					// Execution address.
					address: contract_instance_address, // Does not affect pool id
					// Caller of the EVM.
					caller: H160(*account_address), // Does not affect pool id
					// Apparent value of the EVM.
					apparent_value: deposit.into(),
				};

				if let Ok(fee) = execution_fee.get_contract_fee(&contract_type) {
					if let Err(_e) = fee_manager.update_fee(fee) {
						return Err(fee_manager.get_fee_used())
					}
				} else {
					return Err(fee_manager.get_fee_used());
				}

				// Prepay Gas for the contract execution. This action will be reverted if TX is
				// failed. In case of deployment only EVM burns gas.
				let gas_limit = READONLY_CALL_DEFAULT_GAS_LIMIT;
				let current_call_depth = 0;
				match executor
					.execute_contract_deployment(
						account_address,
						scheme,
						context,
						cluster_address,
						access_type as i8,
						&contract_code,
						account_nonce,
						gas_limit,
						deposit,
						&transaction_hash,
						block_number,
						block_hash,
						block_timestamp,
						current_call_depth,
						updated_state,
						event_tx,
					)
				{
					Ok((_, burnt_gas)) => {
						if let Err(_e) = fee_manager.update_fee(execution_fee.convert_from_gas_to_fee(burnt_gas)) {
							return Err(fee_manager.get_fee_used())
						}
						Ok(fee_manager.get_fee_used())
					},
					Err(_) => return Err(fee_manager.get_fee_used()),
				}
			},
			TransactionType::SmartContractInit {
				contract_code_address,
				arguments,
				deposit
			} => {
				let contract = match updated_state.get_contract(&contract_code_address) {
						Ok(v) => v,
						Err(_e) => return Err(fee_manager.get_fee_used()),
				};
				let contract_type = match contract.r#type.try_into() {
					Ok(v) => v,
					Err(_e) => return Err(fee_manager.get_fee_used()),
				};
				let executor: Box<dyn ContractExecutor + Send + Sync> = match contract_type {
					ContractType::L1XVM => Box::new(L1XVMExecutor),
					ContractType::EVM => Box::new(EVMExecutor),
					ContractType::XTALK => Box::new(XTalkExecutor),
				};
				let context = Context {
					// Execution address.
					address: H160(*account_address),
					// Caller of the EVM.
					caller: H160(*account_address), /* This is where the vault address is taken
					                                 * for pool id */
					// Apparent value of the EVM.
					apparent_value: deposit.into(),
				};

				if let Ok(fee) = execution_fee.get_contract_fee(&contract_type) {
					if let Err(_e) = fee_manager.update_fee(fee) {
						return Err(fee_manager.get_fee_used())
					}
				} else {
					return Err(fee_manager.get_fee_used());
				}

				let gas_limit = READONLY_CALL_DEFAULT_GAS_LIMIT;

				let current_call_depth = 0;
				match executor
					.execute_contract_init(
						account_address,
						context,
						gas_limit,
						deposit,
						cluster_address,
						&contract,
						arguments,
						account_nonce,
						&transaction_hash,
						block_number,
						block_hash,
						block_timestamp,
						current_call_depth,
						updated_state,
						event_tx.clone(),
					)
				{
					Ok((_contract_instance, burnt_gas)) => {
						if let Err(e) = fee_manager.update_fee(execution_fee.convert_from_gas_to_fee(burnt_gas)) {
							return Err(fee_manager.get_fee_used())
						}
						Ok(fee_manager.get_fee_used())
					},
					Err(e) => return Err(fee_manager.get_fee_used()),
				}
			},
			TransactionType::SmartContractFunctionCall {
				contract_instance_address,
				function,
				arguments,
				deposit,
			} => {
				let is_valid_contract_address = match updated_state.is_valid_contract_instance(&contract_instance_address) {
					Ok(valid) => valid,
					Err(_e) => return Err(fee_manager.get_fee_used()),
				};
				if !is_valid_contract_address {
					// This is the case of native token transfer in evm

					if let Err(_e) = fee_manager.update_fee(execution_fee.get_token_transfer_fee()) {
						return Err(fee_manager.get_fee_used())
					}
					Ok(fee_manager.get_fee_used())

				} else {
					// function call execution
					let contract_instance = match updated_state.get_contract_instance(&contract_instance_address) {
						Ok(v) => v,
						Err(_e) => return Err(fee_manager.get_fee_used()),
					};
					let contract = match updated_state.get_contract(&contract_instance.contract_address) {
							Ok(v) => v,
							Err(e) => return Err(fee_manager.get_fee_used()),
					};
					let contract_type = match contract.r#type.try_into() {
						Ok(v) => v,
						Err(_e) => return Err(fee_manager.get_fee_used()),
					};
					let executor: Box<dyn ContractExecutor + Send + Sync> = match contract_type {
						ContractType::L1XVM => Box::new(L1XVMExecutor),
						ContractType::EVM => Box::new(EVMExecutor),
						ContractType::XTALK => Box::new(XTalkExecutor),
					};
					let context = Context {
						// Execution address.
						address: H160(contract_instance_address),
						// Caller of the EVM.
						caller: H160(*account_address), // This is where the vault address is taken for pool id
						// Apparent value of the EVM.
						apparent_value: deposit.into(),
					};

					if let Ok(fee) = execution_fee.get_contract_fee(&contract_type) {
						if let Err(e) = fee_manager.update_fee(fee) {
							return Err(fee_manager.get_fee_used())
						}
					} else {
						return Err(fee_manager.get_fee_used());
					}

					let gas_limit = READONLY_CALL_DEFAULT_GAS_LIMIT;
					let current_call_depth = 0;

					let (_result, burnt_gas) = executor
						.execute_contract_function_call(
							account_address,
							context,
							gas_limit,
							deposit,
							&cluster_address,
							&contract,
							&contract_instance,
							&function,
							&arguments,
							account_nonce,
							&transaction_hash,
							false,
							block_number,
							block_hash,
							block_timestamp,
							current_call_depth,
							updated_state,
							event_tx,
						);
					if let Err(_e) = fee_manager.update_fee(execution_fee.convert_from_gas_to_fee(burnt_gas)) {
						return Err(fee_manager.get_fee_used())
					}
					Ok(fee_manager.get_fee_used())
				}
			},
			TransactionType::CreateStakingPool {
				contract_instance_address,
				min_stake,
				max_stake,
				min_pool_balance,
				max_pool_balance,
				staking_period,
			} => {
				if let Err(e) = fee_manager.update_fee(execution_fee.get_create_staking_pool_fee()) {
					return Err(fee_manager.get_fee_used())
				}

				let execute_staking = ExecuteStaking {};
				match execute_staking
					.execute_native_staking_create_pool(
						account_address,
						cluster_address,
						account_nonce,
						block_number,
						contract_instance_address,
						min_stake,
						max_stake,
						min_pool_balance,
						max_pool_balance,
						staking_period,
						updated_state,
					)
				{
					Ok(_address) => Ok(fee_manager.get_fee_used()),
					Err(_e) => Err(fee_manager.get_fee_used()),
				}
			},
			TransactionType::Stake { pool_address, amount } => {
				if let Err(_e) = fee_manager.update_fee(execution_fee.get_stake_fee()) {
					return Err(fee_manager.get_fee_used())
				}
				let execute_staking = ExecuteStaking {};
				match execute_staking
					.execute_native_staking_stake(
						&pool_address,
						account_address,
						block_number,
						amount,
						updated_state,
					)
				{
					Ok(_) => Ok(fee_manager.get_fee_used()),
					Err(_e) => Err(fee_manager.get_fee_used()),
				}
			},
			TransactionType::UnStake { pool_address, amount } => {
				if let Err(_e) = fee_manager.update_fee(execution_fee.get_unstake_fee()) {
					return Err(fee_manager.get_fee_used())
				}
				let execute_staking = ExecuteStaking {};
				match execute_staking
					.execute_native_staking_un_stake(
						&pool_address,
						account_address,
						block_number,
						amount,
						updated_state,
					)
				{
					Ok(_) => Ok(fee_manager.get_fee_used()),
					Err(_e) => Err(fee_manager.get_fee_used()),
				}
			},
			TransactionType::StakingPoolContract { pool_address, contract_instance_address } => {
				if let Err(_e) = fee_manager.update_fee(execution_fee.get_staking_pool_contract_fee()) {
					return Err(fee_manager.get_fee_used())
				}

				let execute_staking = ExecuteStaking {};
				match execute_staking
					.execute_native_staking_update_contract(
						&pool_address,
						&contract_instance_address,
						account_address,
						block_number,
						updated_state,
					)
				{
					Ok(_) => Ok(fee_manager.get_fee_used()),
					Err(_e) => Err(fee_manager.get_fee_used()),
				}
			},
		}
	}

	fn analyze_reason(
		reason: &ExitReason,
		return_data: Vec<u8>,
	) -> (anyhow::Result<()>, Option<Vec<u8>>) {
		match reason {
			ExitReason::Succeed(s) => {
				info!("analyse_reason: ExitReason::Succeed: {:?}", s);
				(Ok(()), Some(return_data))
			},
			ExitReason::Error(e) => {
				info!("analyse_reason: ExitReason::Error: {:?}", e);
				(Err(anyhow::anyhow!("{:?}", e)), Some(return_data))
			},
			ExitReason::Revert(e) => {
				info!("analyse_reason: ExitReason::Revert: {:?}", e);
				(Err(anyhow::anyhow!("{:?}", e)), Some(return_data))
			},
			ExitReason::Fatal(e) => {
				info!("analyse_reason: ExitReason::Fatal: {:?}", e);
				(Err(anyhow::anyhow!("{:?}", e)), Some(return_data))
			},
		}
	}
}
