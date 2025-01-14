use account::account_state::AccountState as SysAccountState;
use alloy_primitives::hex;
use block::block_state::BlockState;
use contract::contract_state::ContractState;
use contract_instance::contract_instance_state::ContractInstanceState;
use db::db::Database;
use ethers::types::{Log, H256};
use event::event_state::EventState;
use evm_runtime::Capture;
use execute::{
	execute_common::ContractExecutor, execute_contract_evm, execute_contract_l1xvm,
	execute_contract_xtalk,
};
use itertools::Itertools;
use l1x_rpc::rpc_model::{self, estimate_fee_request::TransactionType as gRPCEstimateFeeTransactionType,
						 submit_transaction_request_v2::TransactionType as gRPCTransactionTypeV2, Account as gRPCAccount,
						 AccountState, Block, BlockV2, BlockV3, CreateAccountRequest, CreateAccountResponse,
						 EstimateFeeRequest, EstimateFeeResponse, GetAccountStateRequest, GetAccountStateResponse,
						 GetBlockByNumberRequest, GetBlockByNumberResponse, GetBlockV2ByNumberResponse,
						 GetBlockV3ByNumberResponse, GetChainStateRequest, GetChainStateResponse, GetCurrentNonceRequest,
						 GetCurrentNonceResponse, GetEventsRequest, GetEventsResponse, GetLatestBlocksRequest,
						 GetLatestBlocksResponse,  GetLatestTransactionsRequest, GetProtocolVersionRequest,
						 GetProtocolVersionResponse,  GetStakeRequest, GetStakeResponse, GetTransactionReceiptRequest,
						 GetTransactionReceiptResponse, GetTransactionV3ReceiptResponse, GetTransactionsByAccountRequest,
						 GetTransactionsByAccountResponse, GetTransactionsV3ByAccountResponse, ImportAccountRequest,
						 ImportAccountResponse, SmartContractReadOnlyCallRequest, SmartContractReadOnlyCallResponse,
						 SubmitTransactionRequestV2, SubmitTransactionResponse, TransactionStatus, GetNodeInfoRequest,
						 GetNodeInfoResponse, GetGenesisBlockRequest, GetGenesisBlockResponse, GenesisBlock,
						 GetCurrentNodeInfoRequest, GetCurrentNodeInfoResponse, CurrentNodeInfo, GetNodeHealthsRequest,
						 GetNodeHealthsResponse, BpForEpoch, GetBpForEpochRequest, GetBpForEpochResponse,
						 GetValidatorsForEpochRequest, GetValidatorsForEpochResponse, ValidatorsForEpoch,
						 GetBlockInfoRequest, GetBlockInfoResponse, BlockInfo, ValidatorDetail, GetRuntimeConfigRequest,
						 GetRuntimeConfigResponse, GetBlockWithDetailsByNumberRequest, GetBlockWithDetailsByNumberResponse,
						 VoteResultShort, ValidatorShort,
};
use l1x_vrf::common::get_signature_from_bytes;
use log::{debug, error, info};
use node_crate::node::{FullNode, mempool_add_transaction};
use node_info::node_info_state::NodeInfoState;
use primitives::*;
use secp256k1::PublicKey;
use staking::staking_state::StakingState;
use state::UpdatedState;
use std::{
	str::FromStr,
	sync::Arc,
	vec,
};
use std::collections::HashMap;
use system::{
	access::AccessType, account::{Account, AccountType}, contract::ContractType, errors::NodeError, transaction::{Transaction as SystemTransaction, TransactionType as SystemTransactionType}, transaction_response::{TransactionResponse, TransactionResponseType},
};
use mint::mint;
use tokio::sync::broadcast;
use tonic::Status;
use util::convert::{to_proto_transaction, to_proto_transaction_v3, to_rpc_node_info, to_rpc_node_health, to_rpc_validator,
					to_rpc_block_header_v3, to_rpc_runtime_staking_info, to_rpc_runtime_config, to_rpc_runtime_deny_config,
};
use util::convert::to_proto_transaction_v2;
use compile_time_config::PROTOCOL_VERSION;
use execute::consts::ESTIMATE_FEE_LIMIT;
use validator::validator_state::ValidatorState;
use block_proposer::block_proposer_state::BlockProposerState;
use l1x_node_health::NodeHealthState;
use system::block::BlockType;
use vote_result::vote_result_state::VoteResultState;

pub type SubmitTransactionStream =
	tokio_stream::wrappers::ReceiverStream<Result<SubmitTransactionResponse, Status>>;
pub type GetEventsStream =
	tokio_stream::wrappers::ReceiverStream<Result<GetEventsResponse, Status>>;

#[derive(Clone)]
pub struct FullNodeService {
	pub node: FullNode,
	pub eth_chain_id: u64,
	pub rpc_disable_estimate_fee: bool
}

impl FullNodeService {
	pub async fn new_account(
		&self,
		req: CreateAccountRequest,
	) -> Result<CreateAccountResponse, NodeError> {
		let account = Account::generate_account(&req.password).map_err(|e| {
			NodeError::AccountCreationError(format!("Failed to create account: {}", e))
		})?;

		let response = CreateAccountResponse {
			account: Some(gRPCAccount {
				private_key: account.private_key.to_string(),
				public_key: account.public_key.to_string(),
				address: account.address.to_string(),
			}),
		};
		Ok(response)
	}

	pub async fn import_account(
		&self,
		req: ImportAccountRequest,
	) -> Result<ImportAccountResponse, NodeError> {
		let account = Account::import_account(&req.private_key, &req.password).map_err(|e| {
			NodeError::AccountCreationError(format!("Failed to create account: {}", e))
		})?;

		let response = ImportAccountResponse {
			account: Some(gRPCAccount {
				private_key: account.private_key.to_string(),
				public_key: account.public_key.to_string(),
				address: account.address.to_string(),
			}),
		};
		Ok(response)
	}

	pub async fn get_account_state(
		&self,
		req: GetAccountStateRequest,
	) -> Result<GetAccountStateResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let account_state = SysAccountState::new(&db_pool_conn)
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;
		let address = address_string_to_bytes(&req.address).await?;

		let account = account_state.get_account(&address).await.map_err(|error| {
			error!("{:?}", error);
			NodeError::AccountFetchError(format!("{address:?}"))
		})?;

		let account_type = match account.account_type {
			AccountType::System => 0,
			AccountType::User => 1,
		};

		let acc_state = AccountState {
			balance: account.balance.to_string(),
			nonce: account.nonce.to_string(),
			account_type,
		};
		let response = GetAccountStateResponse { account_state: Some(acc_state) };

		Ok(response)
	}

	/// Submit a transaction to the chain
	pub async fn submit_transaction(
		&self,
		req: SubmitTransactionRequestV2,
	) -> Result<SubmitTransactionStream, NodeError> {
		let (tx_channel, rx) = tokio::sync::mpsc::channel(4);
		let service = self;
		{
			let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
				NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
			})?;
			debug!("SIGNATURE: {:?}", req.signature);
			let signature = get_signature_from_bytes(&req.signature)
				.map_err(|e| NodeError::DBError(format!("Failed to parse signature: {}", e)))?;

			let verifying_key = match PublicKey::from_slice(&req.verifying_key) {
				Ok(key) => key,

				Err(e) =>
					return Err(NodeError::ParseError(format!(
						"Failed to parse verifying_key: {}",
						e
					))),
			};
			let from_address: Address = Account::address(&verifying_key.serialize().to_vec())
				.map_err(|_| {
					NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
				})?;

			let mut contract_code = vec![];
			let nonce: Nonce = req
				.nonce
				.parse()
				.map_err(|_| NodeError::ParseError("Failed to parse nonce".to_string()))?;
			let fee_limit: Balance = req
				.fee_limit
				.parse()
				.map_err(|_| NodeError::ParseError("Failed to parse fee_limit".to_string()))?;

			let transaction_type = req
				.transaction_type
				.ok_or(NodeError::InvalidTransactionType(format!("Must have a TransactionType")))?;

			let tx_type: SystemTransactionType = match transaction_type {
				gRPCTransactionTypeV2::NativeTokenTransfer(tx) => {
					let tx_amount = u128::from_str(&tx.amount)
						.map_err(|_| NodeError::ParseError("Failed to parse amount".to_string()))?;
				
					let to_address: Address = tx.address.try_into().map_err(|_| {
						NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
					})?;

					info!("RPC: NativeTokenTransfer: from={}, to={}, tx.amount={}",
						hex::encode(&from_address), hex::encode(&to_address), tx.amount);

					SystemTransactionType::NativeTokenTransfer(to_address, tx_amount as u128)
				},
				gRPCTransactionTypeV2::SmartContractDeployment(payload) => {
					let deposit = Balance::from_str(&payload.deposit)
						.map_err(|_| NodeError::ParseError("Failed to parse deposit".to_string()))?;
					let access_type = match payload.access_type {
						x if x == AccessType::PRIVATE as i32 => AccessType::PRIVATE,
						x if x == AccessType::PUBLIC as i32 => AccessType::PUBLIC,
						x if x == AccessType::RESTICTED as i32 => AccessType::RESTICTED,
						x =>
							return Err(NodeError::InvalidAccessType(format!(
								"failed to convert AccessType {}",
								x
							))),
					};
					let contract_type = match payload.contract_type {
						x if x == ContractType::L1XVM as i32 => ContractType::L1XVM,
						x if x == ContractType::XTALK as i32 => ContractType::XTALK,
						x if x == ContractType::EVM as i32 => ContractType::EVM,
						x =>
							return Err(NodeError::InvalidContractType(format!(
								"failed to convert ContractType {}",
								x
							))),
					};
					contract_code = payload.contract_code.clone();
					SystemTransactionType::SmartContractDeployment {
						access_type,
						contract_type,
						contract_code: payload.contract_code,
						deposit,
						salt: payload.salt,
					}
				},
				gRPCTransactionTypeV2::SmartContractInit(payload) => {
					let deposit = Balance::from_str(&payload.deposit)
						.map_err(|_| NodeError::ParseError("Failed to parse deposit".to_string()))?;
					let contract_code_address = payload.contract_code_address.try_into().map_err(|_| {
						NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
					})?;
					SystemTransactionType::SmartContractInit {
						contract_code_address, 
						arguments: payload.arguments,
						deposit,
					}
				},
				gRPCTransactionTypeV2::SmartContractFunctionCall(payload) => {
					let deposit = Balance::from_str(&payload.deposit)
					.map_err(|_| NodeError::ParseError("Failed to parse deposit".to_string()))?;
					let contract_instance_address =
						payload.contract_instance_address.try_into().map_err(|_| {
							NodeError::InvalidAddress(
								"Failed to convert bytes to Address".to_string(),
							)
						})?;
					SystemTransactionType::SmartContractFunctionCall {
						contract_instance_address,
						function: payload.function_name,
						arguments: payload.arguments,
						deposit,
					}
				},
				gRPCTransactionTypeV2::CreateStakingPool(payload) => {
					let contract_instance_address: Option<Address> = payload
						.contract_instance_address
						.map(|address_bytes| {
							address_bytes.try_into().map_err(|_| {
								NodeError::InvalidAddress(
									"Failed to convert bytes to Address".to_string(),
								)
							})
						})
						.transpose()?;

					// TODO: handle unwraps
					SystemTransactionType::CreateStakingPool {
						contract_instance_address,
						min_stake: payload
							.min_stake
							.as_ref()
							.map(|s| u128::from_str(s))
							.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
							.map_err(|_| {
								NodeError::ParseError("Failed to parse min_stake".to_string())
							})?,
						max_stake: payload
							.max_stake
							.as_ref()
							.map(|s| u128::from_str(s))
							.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
							.map_err(|_| {
								NodeError::ParseError("Failed to parse max_stake".to_string())
							})?,
						min_pool_balance: payload
							.min_pool_balance
							.as_ref()
							.map(|s| u128::from_str(s))
							.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
							.map_err(|_| {
								NodeError::ParseError(
									"Failed to parse min_pool_balance".to_string(),
								)
							})?,
						max_pool_balance: payload
							.max_pool_balance
							.as_ref()
							.map(|s| u128::from_str(s))
							.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
							.map_err(|_| {
								NodeError::ParseError(
									"Failed to parse max_pool_balance".to_string(),
								)
							})?,
						staking_period: payload
							.staking_period
							.as_ref()
							.map(|s| u128::from_str(s))
							.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
							.map_err(|_| {
								NodeError::ParseError("Failed to parse staking_period".to_string())
							})?,
					}
				},
				gRPCTransactionTypeV2::Stake(payload) => {
					// let pool_address = payload.pool_address.try_into().map_err(|_| {
					// 	NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
					// })?;
					//
					// SystemTransactionType::Stake { pool_address, amount: payload.amount as u128 }
					let pool_address = payload.pool_address.try_into().map_err(|_| {
						NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
					})?;
					let amount = u128::from_str(&payload.amount).map_err(|_| {
						NodeError::ParseError("Failed to parse stake amount".to_string())
					})?;

					SystemTransactionType::Stake { pool_address, amount }
				},
				gRPCTransactionTypeV2::Unstake(payload) => {
					let pool_address = payload.pool_address.try_into().map_err(|_| {
						NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
					})?;
					let amount = u128::from_str(&payload.amount).map_err(|_| {
						NodeError::ParseError("Failed to parse unstake amount".to_string())
					})?;

					SystemTransactionType::UnStake { pool_address, amount }
					// let pool_address = payload.pool_address.try_into().map_err(|_| {
					// 	NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
					// })?;
					//
					// SystemTransactionType::UnStake { pool_address, amount: payload.amount as u128
					// }
				},
			};

			// Form transaction into expected format
			let transaction =
				SystemTransaction::new(nonce, tx_type, fee_limit, signature, verifying_key);
			
			validate::validate_common::ValidateCommon::validate_tx(&transaction, &from_address, &db_pool_conn)
				.await
				.map_err(|e| {
					NodeError::UnexpectedError(format!("Invalid transaction: {:?}", e))
				})?;

			// Submit transaction to the mempool
			mempool_add_transaction(service.node.mempool_tx.clone(), transaction.clone()).await?;

			match TransactionResponse::new(
				transaction.clone(),
				&service.node.cluster_address,
				&contract_code,
				transaction.nonce,
				from_address,
			) {
				Ok(tx_response) => {
					let hash = hex::encode(tx_response.tx_hash);
					let contract_address = match tx_response.data {
						Some(tx_type) => match tx_type {
							TransactionResponseType::SmartContractDeployment(addr) =>
								Some(hex::encode(addr)),
							TransactionResponseType::SmartContractInit(addr) =>
								Some(hex::encode(addr)),
							TransactionResponseType::XtalkSmartContractDeployment(addr) =>
								Some(hex::encode(addr)),
							TransactionResponseType::XtalkSmartContractInit(addr) =>
								Some(hex::encode(addr)),
							TransactionResponseType::CreateStakingPool(addr) =>
								Some(hex::encode(addr)),
						},
						None => None,
					};
					let response = SubmitTransactionResponse { hash, contract_address };
					if tx_channel.send(Ok(response)).await.is_err() {
						return Err(NodeError::UnexpectedError(format!(
							"Event receiver has dropped"
						)));
					}
				},
				Err(e) => {
					return Err(NodeError::UnexpectedError(format!(
						"Something went wrong: {:?}",
						e
					)));
				},
			}
		};
		Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
	}

	/// Submit a transaction to the chain
	pub async fn estimate_fee(
		&self,
		req: EstimateFeeRequest,
	) -> Result<EstimateFeeResponse, NodeError> {
		if self.rpc_disable_estimate_fee {
			return Err(NodeError::UnexpectedError("estimate_fee rpc is disabled".to_string()))
		}

		let (event_tx, _) = broadcast::channel(1000);

		let requested_fee_limit = u128::from_str(&req.fee_limit)
			.map_err(|_| NodeError::ParseError("Failed to parse fee limit".to_string()))?;

		let fee_limit = requested_fee_limit.min(ESTIMATE_FEE_LIMIT);

		let verifying_key = match PublicKey::from_slice(&req.verifying_key) {
			Ok(key) => key,
			Err(e) =>
				return Err(NodeError::ParseError(format!("Failed to parse verifying_key: {}", e))),
		};
		let from_address: Address =
			Account::address(&verifying_key.serialize().to_vec()).map_err(|_| {
				NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
			})?;

		let transaction_type = req
			.transaction_type
			.ok_or(NodeError::InvalidTransactionType(format!("Must have a TransactionType")))?;

		let db_pool_conn =
			Database::get_pool_connection().await.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e.to_string())))?;

		let tx_type: SystemTransactionType = match transaction_type {
			gRPCEstimateFeeTransactionType::NativeTokenTransfer(tx) => {
				let to_address: Address = tx.address.clone().try_into().map_err(|_| {
					NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
				})?;

				let account_state = SysAccountState::new(&db_pool_conn)
					.await
					.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;
				let balance = account_state.get_balance(&from_address).await.map_err(|e| {
					NodeError::AccountFetchError(format!(
						"Failed to fetch balance for account 0x{}: {}",
						hex::encode(from_address),
						e
					))
				})?;

				let tx_amount = u128::from_str(&tx.amount)
					.map_err(|_| NodeError::ParseError("Failed to parse amount".to_string()))?;

				match mint::is_mint(&from_address, &to_address) {
					Ok(true) => (),
					_ =>
						if balance < tx_amount {
							info!("balance less");
							return Err(NodeError::InsufficientBalance(format!(
								"Insufficient funds for account 0x{}, balance: {}",
								hex::encode(from_address),
								balance
							)))
						},
				};

				SystemTransactionType::NativeTokenTransfer(to_address, tx_amount as u128)
			},
			gRPCEstimateFeeTransactionType::SmartContractDeployment(payload) => {
				let access_type = match payload.access_type {
					x if x == AccessType::PRIVATE as i32 => AccessType::PRIVATE,
					x if x == AccessType::PUBLIC as i32 => AccessType::PUBLIC,
					x if x == AccessType::RESTICTED as i32 => AccessType::RESTICTED,
					x =>
						return Err(NodeError::InvalidAccessType(format!(
							"failed to convert AccessType {}",
							x
						))),
				};
				let contract_type = match payload.contract_type {
					x if x == ContractType::L1XVM as i32 => ContractType::L1XVM,
					x if x == ContractType::XTALK as i32 => ContractType::XTALK,
					x if x == ContractType::EVM as i32 => ContractType::EVM,
					x =>
						return Err(NodeError::InvalidContractType(format!(
							"failed to convert ContractType {}",
							x
						))),
				};
				let deposit = Balance::from_str(&payload.deposit)
					.map_err(|_| NodeError::ParseError("Failed to parse deposit".to_string()))?;
				SystemTransactionType::SmartContractDeployment {
					access_type,
					contract_type,
					contract_code: payload.contract_code,
					deposit,
					salt: payload.salt,
				}
			},
			gRPCEstimateFeeTransactionType::SmartContractInit(payload) => {
				let deposit = Balance::from_str(&payload.deposit)
					.map_err(|_| NodeError::ParseError("Failed to parse deposit".to_string()))?;
				let contract_code_address = payload.contract_code_address.try_into().map_err(|_| {
					NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
				})?;
				SystemTransactionType::SmartContractInit {
					contract_code_address,
					arguments: payload.arguments,
					deposit,
				}
			},
			gRPCEstimateFeeTransactionType::SmartContractFunctionCall(payload) => {
				let deposit = Balance::from_str(&payload.deposit)
					.map_err(|_| NodeError::ParseError("Failed to parse deposit".to_string()))?;
				let contract_instance_address =
					payload.contract_instance_address.try_into().map_err(|_| {
						NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
					})?;
				let function: Vec<u8> = payload.function_name;
				let arguments: Vec<u8> = payload.arguments;
				SystemTransactionType::SmartContractFunctionCall {
					contract_instance_address,
					function,
					arguments,
					deposit,
				}
			},
			gRPCEstimateFeeTransactionType::CreateStakingPool(payload) => {
				let contract_instance_address: Option<Address> = payload
					.contract_instance_address
					.map(|address_bytes| {
						address_bytes.try_into().map_err(|_| {
							NodeError::InvalidAddress(
								"Failed to convert bytes to Address".to_string(),
							)
						})
					})
					.transpose()?;

				SystemTransactionType::CreateStakingPool {
					contract_instance_address,
					min_stake: payload
						.min_stake
						.as_ref()
						.map(|s| u128::from_str(s))
						.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
						.map_err(|_| {
							NodeError::ParseError("Failed to parse min_stake".to_string())
						})?,
					max_stake: payload
						.max_stake
						.as_ref()
						.map(|s| u128::from_str(s))
						.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
						.map_err(|_| {
							NodeError::ParseError("Failed to parse max_stake".to_string())
						})?,
					min_pool_balance: payload
						.min_pool_balance
						.as_ref()
						.map(|s| u128::from_str(s))
						.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
						.map_err(|_| {
							NodeError::ParseError("Failed to parse min_pool_balance".to_string())
						})?,
					max_pool_balance: payload
						.max_pool_balance
						.as_ref()
						.map(|s| u128::from_str(s))
						.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
						.map_err(|_| {
							NodeError::ParseError("Failed to parse max_pool_balance".to_string())
						})?,
					staking_period: payload
						.staking_period
						.as_ref()
						.map(|s| u128::from_str(s))
						.transpose() // Convert Option<Result<_, _>> to Result<Option<_>, _>
						.map_err(|_| {
							NodeError::ParseError("Failed to parse staking_period".to_string())
						})?,
				}
			},
			gRPCEstimateFeeTransactionType::Stake(payload) => {
				let pool_address = payload.pool_address.try_into().map_err(|_| {
					NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
				})?;
				let amount = u128::from_str(&payload.amount).map_err(|_| {
					NodeError::ParseError("Failed to parse stake amount".to_string())
				})?;

				SystemTransactionType::Stake { pool_address, amount }
			},
			gRPCEstimateFeeTransactionType::Unstake(payload) => {
				let pool_address = payload.pool_address.try_into().map_err(|_| {
					NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
				})?;
				let amount = u128::from_str(&payload.amount).map_err(|_| {
					NodeError::ParseError("Failed to parse unstake amount".to_string())
				})?;

				SystemTransactionType::UnStake { pool_address, amount }
			},
		};

		let dummy_signature_bytes = hex!("e789de90be515bca202f184259ea1a16a7badc2e01ef1a6efa50a486175d32532123df702e3aeeb9ebaa292ddb8b07346cb4cff9aa4ed178094eeb0687415f11");
		let signature = get_signature_from_bytes(&dummy_signature_bytes)
			.map_err(|e| NodeError::ParseError(format!("Failed to parse dummy signature: {}", e)))?;

		let nonce = {
			let account_state = SysAccountState::new(&db_pool_conn)
				.await
				.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;

			account_state.get_nonce(&from_address).await.map_err(|e| {
				NodeError::AccountFetchError(format!(
					"Failed to fetch nonce for account 0x{}: {}",
					hex::encode(from_address),
					e
				))
			})?
		};

		let transaction =
			SystemTransaction::new(nonce.checked_add(1).unwrap_or(Nonce::MAX), tx_type, fee_limit, signature, verifying_key);

		let cluster_address = &self.node.cluster_address;
		let latest_block_header = {
			let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;
			block_state.block_head_header(cluster_address.clone()).await.map_err(|e| NodeError::BlockFetchError(format!("Failed to fetch the block head header: {}", e)))?
		};

		info!("RPC: EstimateFee");

		let mut fees: Balance = 0;
		let account_address = Account::address(&transaction.verifying_key).map_err(|e| NodeError::InvalidAddress(format!("Failed to get address: {}", e)))?;
		if let Err(e) = execute::execute_block::ExecuteBlock::execute_estimate_fee(
			&transaction,
			cluster_address,
			latest_block_header.block_number,
			&latest_block_header.block_hash,
			latest_block_header.timestamp,
			Arc::new(&db_pool_conn),
			event_tx,
			account_address,
			&mut fees,
		)
		.await {
			return Err(NodeError::UnexpectedError(format!("Failed to call estimate fee function {e:?}")));
		}

		Ok(EstimateFeeResponse { fee: fees.to_string() })
	}

	pub async fn get_transaction_receipt(
		&self,
		req: GetTransactionReceiptRequest,
	) -> Result<rpc_model::GetTransactionReceiptResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn)
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;
		let hash_string = req.hash.as_str().trim_start_matches("0x");
		let tx_resp = block_state
			.load_transaction_receipt(parse_hash(&hash_string).await?, self.node.cluster_address)
			.await
			.map_err(|e| {
				NodeError::TransactionReceiptFetchError(format!(
					"Failed to load transaction receipt by hash {}: {}",
					req.hash, e
				))
			})?
			.ok_or(NodeError::TransactionReceiptFetchError(format!(
				"No transaction receipt for hash {}:",
				req.hash
			)))?;

		let tx = to_proto_transaction(tx_resp.transaction)?;
		let from = tx_resp.from.to_vec();
		let transaction_hash = tx_resp.tx_hash.to_vec();
		let block_hash = tx_resp.block_hash.to_vec();
		let block_number = i64::try_from(tx_resp.block_number).unwrap_or(i64::MAX);
		let fee_used = tx_resp.fee_used.to_string();
		let timestamp = u64::try_from(tx_resp.timestamp).unwrap_or(u64::MIN);

		let tx = rpc_model::TransactionResponse {
			transaction: Some(tx),
			from,
			transaction_hash,
			block_hash,
			block_number,
			fee_used,
			timestamp,
		};

		let response = GetTransactionReceiptResponse {
			transaction: Some(tx),
			status: if tx_resp.status {
				TransactionStatus::Succeed.into()
			} else {
				TransactionStatus::Failed.into()
			},
		};

		Ok(response)
	}

	pub async fn get_transaction_v3_receipt(
		&self,
		req: GetTransactionReceiptRequest,
	) -> Result<rpc_model::GetTransactionV3ReceiptResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn)
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;
		let hash_string = req.hash.as_str().trim_start_matches("0x");
		let tx_resp = block_state
			.load_transaction_receipt(parse_hash(&hash_string).await?, self.node.cluster_address)
			.await
			.map_err(|e| {
				NodeError::TransactionReceiptFetchError(format!(
					"Failed to load transaction receipt by hash {}: {}",
					req.hash, e
				))
			})?
			.ok_or(NodeError::TransactionReceiptFetchError(format!(
				"No transaction receipt for hash {}:",
				req.hash
			)))?;

		let tx = to_proto_transaction_v3(tx_resp.transaction)?;
		let from = tx_resp.from.to_vec();
		let transaction_hash = tx_resp.tx_hash.to_vec();
		let block_hash = tx_resp.block_hash.to_vec();
		let block_number = i64::try_from(tx_resp.block_number).unwrap_or(i64::MAX);
		let fee_used = tx_resp.fee_used.to_string();
		let timestamp = u64::try_from(tx_resp.timestamp).unwrap_or(u64::MIN);

		let tx = rpc_model::TransactionV3Response {
			transaction: Some(tx),
			from,
			transaction_hash,
			block_hash,
			block_number,
			fee_used,
			timestamp,
		};

		let response = GetTransactionV3ReceiptResponse {
			transaction: Some(tx),
			status: if tx_resp.status {
				TransactionStatus::Succeed.into()
			} else {
				TransactionStatus::Failed.into()
			},
		};

		Ok(response)
	}

	pub async fn get_transactions_by_account(
		&self,
		req: GetTransactionsByAccountRequest,
	) -> Result<GetTransactionsByAccountResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let address = address_string_to_bytes(&req.address).await?;
		let min_block = req.starting_from;
		let num_transactions: usize = req.number_of_transactions as usize;

		let tx_vec: Vec<system::transaction_receipt::TransactionReceiptResponse> = block_state
			.load_transaction_receipt_by_address(address)
			.await
			.map_err(|e| NodeError::AccountFetchError(format!("Account may not exist: {}", e)))?;

		// Get only sorted transactions >= to the min requested block # and take the latest
		// num_transactions by block number
		let tx_vec_filtered: Vec<system::transaction_receipt::TransactionReceiptResponse> = tx_vec
			.into_iter()
			.filter(|tx| tx.block_number >= min_block as u128)
			.sorted_by_key(|tx| tx.block_number)
			.rev()
			.take(num_transactions)
			.collect();

		let mut proto_transactions: Vec<rpc_model::TransactionResponse> = vec![];
		for tx_receipt in tx_vec_filtered {
			let tx = to_proto_transaction(tx_receipt.transaction)?;
			let from = tx_receipt.from.to_vec();
			let transaction_hash = tx_receipt.tx_hash.to_vec();
			let block_hash = tx_receipt.block_hash.to_vec();
			let block_number = i64::try_from(tx_receipt.block_number).unwrap_or(i64::MAX);
			let fee_used = tx_receipt.fee_used.to_string();
			let timestamp = u64::try_from(tx_receipt.timestamp).unwrap_or(u64::MIN);

			let tx = rpc_model::TransactionResponse {
				transaction: Some(tx),
				from,
				transaction_hash,
				block_hash,
				block_number,
				fee_used,
				timestamp,
			};

			// let tx = to_proto_transaction(tx_receipt.transaction).await?;
			proto_transactions.push(tx);
		}

		let response = GetTransactionsByAccountResponse { transactions: proto_transactions };

		Ok(response)
	}

	pub async fn get_transactions_v3_by_account(
		&self,
		req: GetTransactionsByAccountRequest,
	) -> Result<GetTransactionsV3ByAccountResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let address = address_string_to_bytes(&req.address).await?;
		let min_block = req.starting_from;
		let num_transactions: usize = req.number_of_transactions as usize;

		let tx_vec: Vec<system::transaction_receipt::TransactionReceiptResponse> = block_state
			.load_transaction_receipt_by_address(address)
			.await
			.map_err(|e| NodeError::AccountFetchError(format!("Account may not exist: {}", e)))?;

		// Get only sorted transactions >= to the min requested block # and take the latest
		// num_transactions by block number
		let tx_vec_filtered: Vec<system::transaction_receipt::TransactionReceiptResponse> = tx_vec
			.into_iter()
			.filter(|tx| tx.block_number >= min_block as u128)
			.sorted_by_key(|tx| tx.block_number)
			.rev()
			.take(num_transactions)
			.collect();

		let mut proto_transactions: Vec<rpc_model::TransactionV3Response> = vec![];
		for tx_receipt in tx_vec_filtered {
			let tx = to_proto_transaction_v3(tx_receipt.transaction)?;
			let from = tx_receipt.from.to_vec();
			let transaction_hash = tx_receipt.tx_hash.to_vec();
			let block_hash = tx_receipt.block_hash.to_vec();
			let block_number = i64::try_from(tx_receipt.block_number).unwrap_or(i64::MAX);
			let fee_used = tx_receipt.fee_used.to_string();
			let timestamp = u64::try_from(tx_receipt.timestamp).unwrap_or(u64::MIN);

			let tx = rpc_model::TransactionV3Response {
				transaction: Some(tx),
				from,
				transaction_hash,
				block_hash,
				block_number,
				fee_used,
				timestamp,
			};

			// let tx = to_proto_transaction(tx_receipt.transaction).await?;
			proto_transactions.push(tx);
		}

		let response = GetTransactionsV3ByAccountResponse { transactions: proto_transactions };

		Ok(response)
	}

	pub async fn smart_contract_read_only_call(
		&self,
		req: SmartContractReadOnlyCallRequest,
	) -> Result<SmartContractReadOnlyCallResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let call = match req.call {
			Some(c) => c,
			None =>
				return Err(NodeError::InvalidReadOnlyFunctionCall(format!(
					"Must contain a SmartContractFunctionCall"
				))),
		};
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;
		let contract_instance_address = call.contract_address.try_into().map_err(|_| {
			NodeError::InvalidAddress("Failed to convert bytes to Address".to_string())
		})?;
		let function_name = call.function_name;
		let args = call.arguments.to_vec();
		let contract_instance_state =
			ContractInstanceState::new(&db_pool_conn).await.map_err(|e| {
				NodeError::DBError(format!(
					"Failed to access ContractInstanceState database: {}",
					e
				))
			})?;
		let contract_state = ContractState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access ContractState database: {}", e))
		})?;

		let account_state = SysAccountState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access AccountState database: {}", e))
		})?;

		let contract_instance = contract_instance_state
			.get_contract_instance(&contract_instance_address)
			.await
			.map_err(|e| NodeError::ContractInstanceFetchError(format!("Failed due to: {}", e)))?;
		let account = account_state
			.get_account(&contract_instance.owner_address)
			.await
			.map_err(|e| NodeError::AccountFetchError(format!("Account does not exist: {}", e)))?;
		let nonce = account.nonce + 1;
		let block_head = block_state.block_head_header(self.node.cluster_address)
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to access block_head database: {}", e)))?;

		EventState::new(&db_pool_conn)
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to access EventState database: {}", e)))
			.and_then(|_event_state| Ok(()))?;

		let contract = contract_state
			.get_contract(&contract_instance.contract_address)
			.await
			.map_err(|e| NodeError::ContractFetchError(format!("Failed due to: {}", e)))?;
		let contract_type = contract.r#type.try_into().map_err(|_| {
			NodeError::InvalidContractType(format!("Invalid ContractType: {}", contract.r#type))
		})?;
		let executor: &(dyn ContractExecutor + Sync) = match contract_type {
			ContractType::L1XVM => &execute_contract_l1xvm::ExecuteContract,
			ContractType::XTALK => &execute_contract_xtalk::ExecuteContract,
			ContractType::EVM => &execute_contract_evm::ExecuteContract,
		};

		// TODO: Future intern refactor - read only calls don't return events, but low level api's
		// require this event channel sender
		let (event_tx, _) = broadcast::channel(1000);

		info!("RPC: ReadOnlyCall: to={}", hex::encode(&contract_instance.instance_address));

		let (tx, _cancel_on_drop) = state::updated_state_db::run_db_handler().await.map_err(|e| {
			NodeError::DBError(format!("Can't start db handler: {}", e))
		})?;
		let (result, _gas) = tokio::task::block_in_place(|| {
			let mut updated_state = UpdatedState::new(tx);
			let current_call_depth = 0;
			let (result, gas) = executor
				.execute_contract_function_call_read_only(
					&contract,
					&contract_instance,
					&function_name,
					&args,
					execute::consts::READONLY_CALL_DEFAULT_GAS_LIMIT,
					&self.node.cluster_address,
					nonce,
					block_head.block_number,
					&block_head.block_hash,
					block_head.timestamp,
					current_call_depth,
					&mut updated_state,
					event_tx,
				);

				(result, gas)
			});

		let result = match result {
			Capture::Exit(reason) => {
				let (_reason, result) = reason;
				(*result).clone()
			},
			Capture::Trap(_resolve) => {
				vec![]
			},
		};
		/*.map_err(|e| {
			NodeError::SmartContractCallFailed(format!("Read only call failed: {}", e))
		})?;*/

		let response =
			SmartContractReadOnlyCallResponse { status: TransactionStatus::Succeed.into(), result };

		Ok(response)
	}

	pub async fn get_chain_state(
		&self,
		_req: GetChainStateRequest,
	) -> Result<GetChainStateResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let cluster_address = self.node.cluster_address;

		let chain_state = block_state.load_chain_state(cluster_address).await.map_err(|e| {
			NodeError::ChainStateFetchError(format!("Failed to load chain state: {}", e))
		})?;

		let response = GetChainStateResponse {
			cluster_address: hex::encode(chain_state.cluster_address),
			head_block_number: chain_state.block_number.to_string(),
			head_block_hash: hex::encode(chain_state.block_hash),
		};

		Ok(response)
	}

	pub async fn get_block_by_number(
		&self,
		req: GetBlockByNumberRequest,
	) -> Result<GetBlockByNumberResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;
		let block_number = parse_block_number(req.block_number).await?;
		let cluster_address = self.node.cluster_address;

		let persisted_block = block_state
			.load_block_response(block_number, &cluster_address)
			.await
			.map_err(|e| NodeError::BlockFetchError(format!("Failed to get block: {}", e)))?;

		let mut transactions: Vec<rpc_model::TransactionResponse> = vec![];
		for txn in persisted_block.transactions.into_iter() {
			let txn = rpc_model::TransactionResponse {
				transaction: Some(to_proto_transaction(txn.transaction)?),
				from: txn.from.to_vec(),
				transaction_hash: txn.tx_hash.to_vec(),
				block_hash: txn.block_hash.to_vec(),
				block_number: i64::try_from(txn.block_number).unwrap_or(i64::MAX),
				fee_used: txn.fee_used.to_string(),
				timestamp: txn.timestamp as u64,
			};

			transactions.push(txn);
		}
		let block = Block {
			number: block_number.to_string(),
			hash: hex::encode(persisted_block.block_header.block_hash),
			parent_hash: hex::encode(persisted_block.block_header.parent_hash),
			timestamp: persisted_block.block_header.timestamp,
			transactions,
			block_type: persisted_block.block_header.block_type as i32,
			cluster_address: hex::encode(cluster_address),
		};

		Ok(GetBlockByNumberResponse { block: Some(block) })
	}

	// Fetch block with transaction version 2
	pub async fn get_block_v2_by_number(
		&self,
		req: GetBlockByNumberRequest,
	) -> Result<GetBlockV2ByNumberResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;
		let block_number = parse_block_number(req.block_number).await?;
		let cluster_address = self.node.cluster_address;

		let persisted_block = block_state
			.load_block_response(block_number, &cluster_address)
			.await
			.map_err(|e| NodeError::BlockFetchError(format!("Failed to get block: {}", e)))?;

		let mut transactions: Vec<rpc_model::TransactionV2Response> = vec![];
		for txn in persisted_block.transactions.into_iter() {
			let txn = rpc_model::TransactionV2Response {
				transaction: Some(to_proto_transaction_v2(txn.transaction)?),
				from: txn.from.to_vec(),
				transaction_hash: txn.tx_hash.to_vec(),
				block_hash: txn.block_hash.to_vec(),
				block_number: i64::try_from(txn.block_number).unwrap_or(i64::MAX),
				fee_used: txn.fee_used.to_string(),
				timestamp: txn.timestamp as u64,
			};

			transactions.push(txn);
		}
		let block = BlockV2 {
			number: block_number.to_string(),
			hash: hex::encode(persisted_block.block_header.block_hash),
			parent_hash: hex::encode(persisted_block.block_header.parent_hash),
			timestamp: persisted_block.block_header.timestamp,
			transactions,
			block_type: persisted_block.block_header.block_type as i32,
			cluster_address: hex::encode(cluster_address),
		};

		Ok(GetBlockV2ByNumberResponse { block: Some(block) })
	}

	// Fetch block with transaction version 3
	pub async fn get_block_v3_by_number(
		&self,
		req: GetBlockByNumberRequest,
	) -> Result<GetBlockV3ByNumberResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;
		let block_number = parse_block_number(req.block_number).await?;
		let cluster_address = self.node.cluster_address;

		let persisted_block = block_state
			.load_block_response(block_number, &cluster_address)
			.await
			.map_err(|e| NodeError::BlockFetchError(format!("Failed to get block: {}", e)))?;

		let mut transactions: Vec<rpc_model::TransactionV3Response> = vec![];
		for txn in persisted_block.transactions.into_iter() {
			let txn = rpc_model::TransactionV3Response {
				transaction: Some(to_proto_transaction_v3(txn.transaction)?),
				from: txn.from.to_vec(),
				transaction_hash: txn.tx_hash.to_vec(),
				block_hash: txn.block_hash.to_vec(),
				block_number: i64::try_from(txn.block_number).unwrap_or(i64::MAX),
				fee_used: txn.fee_used.to_string(),
				timestamp: txn.timestamp as u64,
			};

			transactions.push(txn);
		}
		let block = BlockV3 {
			number: block_number.to_string(),
			hash: hex::encode(persisted_block.block_header.block_hash),
			parent_hash: hex::encode(persisted_block.block_header.parent_hash),
			timestamp: persisted_block.block_header.timestamp,
			transactions,
			block_type: persisted_block.block_header.block_type as i32,
			cluster_address: hex::encode(cluster_address),
			state_hash: hex::encode(persisted_block.block_header.state_hash),
			block_version: persisted_block.block_header.block_version.to_string(),
			epoch: persisted_block.block_header.epoch.to_string(),
		};

		Ok(GetBlockV3ByNumberResponse { block: Some(block) })
	}

	/// Get the latest n number of block headers
	pub async fn get_latest_block_headers(
		&self,
		num_headers: u32,
	) -> Result<Vec<rpc_model::BlockHeaderV3>, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let block_headers = block_state
			.load_latest_block_headers(num_headers, self.node.cluster_address)
			.await
			.map_err(|e| {
				NodeError::BlockFetchError(format!("Failed to get latest block headers: {}", e))
			})?;

		Ok(block_headers)
	}

	pub async fn get_latest_block_headers_v2(
		&self,
		num_headers: u32,
	) -> Result<Vec<rpc_model::BlockHeaderV3>, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let block_headers = block_state
			.load_latest_block_headers(num_headers, self.node.cluster_address)
			.await
			.map_err(|e| {
				NodeError::BlockFetchError(format!("Failed to get latest block headers: {}", e))
			})?;

		Ok(block_headers)
	}

	pub async fn get_latest_transactions(
		&self,
		req: GetLatestTransactionsRequest,
	) -> Result<(), NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let num_transactions = req.number_of_transactions;

		// let tx_per_page: usize = req.transactions_per_page.try_into().unwrap_or(usize::MAX);

		block_state.load_latest_transactions(num_transactions).await.map_err(|e| {
			NodeError::BlockFetchError(format!("Failed to get latest transactions: {}", e))
		})?;

		Ok(())
	}

	pub async fn get_stake(&self, req: GetStakeRequest) -> Result<GetStakeResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let staking_state = StakingState::new(&db_pool_conn)
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;

		let address = address_string_to_bytes(&req.account_address).await?;
		let pool_address = address_string_to_bytes(&req.pool_address).await?;

		let staking_account =
			staking_state.get_staking_account(&address, &pool_address).await.map_err(|e| {
				NodeError::StakeFetchError(format!("Failed to get staking account: {}. This likely means that the account has no stake in this pool i.e. a staking balance of 0.", e))
			})?;

		let response = GetStakeResponse { amount: staking_account.balance.to_string() };

		Ok(response)
	}

	/// Get the current nonce value for the provided account address
	pub async fn get_current_nonce(
		&self,
		req: GetCurrentNonceRequest,
	) -> Result<GetCurrentNonceResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let account_state = SysAccountState::new(&db_pool_conn)
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;
		let address = address_string_to_bytes(&req.address).await?;

		let account = account_state
			.get_account(&address)
			.await
			.map_err(|e| NodeError::AccountFetchError(format!("Account does not exist: {}", e)))?;

		let response = GetCurrentNonceResponse { nonce: u128::to_string(&account.nonce) };

		Ok(response)
	}

	/// Get event(s) emitted from a transaction, by hash
	pub async fn get_events(&self, req: GetEventsRequest) -> Result<GetEventsStream, NodeError> {
		let (tx, rx) = tokio::sync::mpsc::channel(4);

		{
			let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
				NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
			})?;
			let event_state = EventState::new(&db_pool_conn).await.map_err(|e| {
				NodeError::DBError(format!("Failed to access events database: {}", e))
			})?;

			let tx_hash = parse_hash(&req.tx_hash).await?;
			let events = event_state
				.get_all_events(&tx_hash)
				.await
				.map_err(|e| NodeError::EventFetchError(format!("Failed to find events: {}", e)))?;

			let response = GetEventsResponse { events_data: events };
			if tx.send(Ok(response)).await.is_err() {
				return Err(NodeError::UnexpectedError(format!("Event receiver has dropped")));
			}
		}

		Ok(tokio_stream::wrappers::ReceiverStream::new(rx))
	}

	// Check if the transaction has been included in a block
	pub async fn monitor_transaction_inclusion(&self, hash: H256) -> Result<bool, NodeError> {
		let hash_string = format!("{:x}", hash);
		let prost_string: ::prost::alloc::string::String = hash_string.into();
		let request = GetEventsRequest { tx_hash: prost_string, timestamp: 0u64 };

		let max_attempts = 200;
		let delay_duration = tokio::time::Duration::from_millis(1000);
		let mut attempts = 0;

		loop {
			if attempts >= max_attempts {
				break; // Exit the loop after max_attempts
			} else {
				match self.get_events(request.clone()).await {
					Ok(event_response) => {
						let response =
							match event_response.into_inner().recv().await.ok_or_else(|| {
								NodeError::ParseError("No events found".to_string())
							})? {
								Ok(response) => response,
								Err(e) => {
									error!("Failed to get events: {}", e);
									continue;
								},
							};

						for event in response.events_data {
							if let Ok(output) = serde_json::from_slice::<serde_json::Value>(&event)
							{
								if let Ok(event_log) = serde_json::from_value::<Log>(output) {
									if event_log.transaction_hash == Some(hash) {
										return Ok(true);
									}
								}
							}
						}
					},
					Err(e) => {
						error!("Failed to get events: {}", e);
					},
				}

				attempts += 1; // Increment the attempt counter
				tokio::time::sleep(delay_duration).await;
			}
		}

		Ok(false) // Return false if the transaction is not found after all attempts
	}

	pub async fn get_latest_blocks(
		&self,
		_req: GetLatestBlocksRequest,
	) -> Result<GetLatestBlocksResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let cluster_address = self.node.cluster_address;

		let chain_state = block_state.load_chain_state(cluster_address).await.map_err(|e| {
			NodeError::ChainStateFetchError(format!("Failed to load chain state: {}", e))
		})?;

		let mut last_executed_block = 0;
		if chain_state.block_number >= 1 {
			let mut block_number = chain_state.block_number;
			while block_number >= 1 {
				if block_state.is_block_executed(block_number, &chain_state.cluster_address ).await.map_err(|e| {
					NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))})? {
					last_executed_block = block_number;
					break;
				}
				block_number -= 1
			}
		}

		let response = GetLatestBlocksResponse {
			cluster_address: hex::encode(chain_state.cluster_address),
			head_block_number: chain_state.block_number.to_string(),
			head_block_hash: hex::encode(chain_state.block_hash),
			last_executed_block: last_executed_block.to_string()
		};

		Ok(response)
	}

	pub async fn get_protocol_version(&self, req: GetProtocolVersionRequest, ) -> Result<GetProtocolVersionResponse, NodeError> {
		let response = GetProtocolVersionResponse {
			protocol_version: PROTOCOL_VERSION
		};
		Ok(response)
	}

	pub async fn get_node_info(
		&self,
		_req: GetNodeInfoRequest,
	) -> Result<GetNodeInfoResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;

        let node_info_state = NodeInfoState::new(&db_pool_conn).await.map_err(|e| {
            NodeError::UnexpectedError(format!("Failed to create NodeInfoState: {}", e))
        })?;

		let staking_state = StakingState::new(&db_pool_conn).await.map_err(|e| {
            NodeError::UnexpectedError(format!("Failed to create StakingState: {}", e))
        })?;

        let system_node_infos = node_info_state.load_nodes(&self.node.cluster_address).await.map_err(|e| {
            NodeError::UnexpectedError(format!("Failed to load node infos: {}", e))
        })?;
		
		let node_info: Vec<l1x_rpc::rpc_model::NodeInfo> = system_node_infos
			.iter()
			.map(|(_key, system_node_info)| to_rpc_node_info(system_node_info))
			.collect::<Result<Vec<_>, _>>()
			.map_err(|e| NodeError::UnexpectedError(format!("Failed to convert node info: {}", e)))?;

        Ok(GetNodeInfoResponse {
            node_info,
        })
	}

    pub async fn get_genesis_block(
        &self,
        _req: GetGenesisBlockRequest,
    ) -> Result<GetGenesisBlockResponse, NodeError> {
        let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
            NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
        })?;
        let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
            NodeError::DBError(format!("Failed to access BlockState database: {}", e))
        })?;

        let (block_number, epoch) = block_state.load_genesis_block_time().await.map_err(|e| {
            NodeError::BlockFetchError(format!("Failed to fetch genesis block: {}", e))
        })?;

        let genesis_block = GenesisBlock {
            block_number: block_number as i64,
            epoch: epoch as i64,
        };

        Ok(GetGenesisBlockResponse {
            genesis_block: Some(genesis_block),
        })
    }

	pub async fn get_current_node_info(
		&self,
		_req: GetCurrentNodeInfoRequest,
	) -> Result<GetCurrentNodeInfoResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::NodeInfoFetchError(format!("Failed to get database connection from pool: {}", e))
		})?;

		// fetch node_info
		let node_info_state = NodeInfoState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::NodeInfoFetchError(format!("Failed to initialize NodeInfoState: {}", e))
		})?;
		let node_info = node_info_state
			.load_node_info(&self.node.node_address)
			.await
			.map_err(|e| NodeError::NodeInfoFetchError(format!("Failed to fetch node info: {}", e)))?;

		let rpc_node_info = CurrentNodeInfo {
			address: hex::encode(node_info.address),
			peer_id: node_info.peer_id,
			ip_address: hex::encode(node_info.data.ip_address)
		};

		// fetch block_detail
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::NodeInfoFetchError(format!("Failed to initialize BlockState: {}", e))
		})?;
		let block_header = block_state
			.block_head_header(self.node.cluster_address)
			.await
			.map_err(|e| { NodeError::NodeInfoFetchError(format!("Failed to fetch block header: {}", e)) })?;
		let block_number = block_header.block_number;
		let epoch = block_header.epoch;

		// fetch block proposer for latest epoch
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::NodeInfoFetchError(format!("Failed to initialize BlockProposerState: {}", e))
		})?;
		let block_proposer = block_proposer_state
			.load_block_proposer(self.node.cluster_address, epoch)
			.await
			.map_err(|e| NodeError::NodeInfoFetchError(format!("Failed to fetch block proposer: {}", e)))?
			.ok_or(NodeError::NodeInfoFetchError(format!("No block proposer available for epoch: {}", epoch)))?;

		// fetch validators for latest block
		let validator_state = ValidatorState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::NodeInfoFetchError(format!("Failed to initialize ValidatorState: {}", e))
		})?;
		let validators = validator_state
			.load_all_validators(epoch)
			.await
			.map_err(|e| NodeError::NodeInfoFetchError(format!("Failed to fetch validators for block {}: {}", block_number, e)))?
			.unwrap_or_else(Vec::new);
		let validators_addresses: Vec<String> = validators
			.iter()
			.map(|validator| hex::encode(validator.address))
			.collect();

		Ok(GetCurrentNodeInfoResponse {
			node_info: Some(rpc_node_info),
			block_number: block_number.to_string(),
			epoch: epoch.to_string(),
			block_proposer: hex::encode(block_proposer.address),
			validators: validators_addresses,
		})
	}

	pub async fn get_node_healths(
		&self,
		req: GetNodeHealthsRequest,
	) -> Result<GetNodeHealthsResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;

		let node_health_state = NodeHealthState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::UnexpectedError(format!("Failed to create NodeHealthState: {}", e))
		})?;

		let node_healths = node_health_state.load_node_healths(req.epoch).await.map_err(|e| {
			NodeError::UnexpectedError(format!("Failed to load node healths: {}", e))
		})?;

		if !node_healths.is_empty() {
			let rpc_node_healths: Vec<l1x_rpc::rpc_model::NodeHealth> = node_healths
				.iter()
				.map(|health| {
					to_rpc_node_health(health).map_err(|e| NodeError::UnexpectedError(format!("Failed to convert node health: {}", e)))
				})
				.collect::<Result<Vec<_>, _>>()?;
			Ok(GetNodeHealthsResponse {
				node_healths: rpc_node_healths,
			})
		} else {
			Err(NodeError::UnexpectedError(format!("No node_health available for this epoch: {}", req.epoch)))
		}
	}

	pub async fn get_block_proposer_for_epoch(
		&self,
		req: GetBpForEpochRequest,
	) -> Result<GetBpForEpochResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::BlockProposerFetchError(format!("Failed to get database connection from pool: {}", e))
		})?;
		let mut response = GetBpForEpochResponse {
			bp_for_epoch: vec![],
		};
		let epoch = req.epoch;

		// fetch block proposer for latest epoch
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::BlockProposerFetchError(format!("Failed to initialize BlockProposerState: {}", e))
		})?;
		let block_proposer = block_proposer_state
			.load_block_proposer(self.node.cluster_address, epoch)
			.await
			.map_err(|e| NodeError::BlockProposerFetchError(format!("Failed to fetch block proposer: {}", e)))?
			.ok_or(NodeError::BlockProposerFetchError(format!("No block proposer available for epoch: {}", epoch)))?;
		let bp_for_epoch = BpForEpoch {
			epoch,
			bp_address: block_proposer.address.to_vec(),
		};

		response.bp_for_epoch.push(bp_for_epoch);

		if epoch > 0 {
			let previous_epoch  = epoch - 1;
			let block_proposer = block_proposer_state
				.load_block_proposer(self.node.cluster_address, previous_epoch )
				.await
				.map_err(|e| NodeError::BlockProposerFetchError(format!("Failed to fetch block proposer: {}", e)))?
				.ok_or(NodeError::BlockProposerFetchError(format!("No block proposer available for epoch: {}", epoch)))?;
			let bp_for_previous_epoch = BpForEpoch {
				epoch: previous_epoch,
				bp_address: block_proposer.address.to_vec(),
			};
			response.bp_for_epoch.push(bp_for_previous_epoch);
		}

		Ok(response)
	}

	pub async fn get_validators_for_epoch(
		&self,
		req: GetValidatorsForEpochRequest,
	) -> Result<GetValidatorsForEpochResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::ValidatorsFetchError(format!("Failed to get database connection from pool: {}", e))
		})?;
		let mut response = GetValidatorsForEpochResponse {
			validators_for_epochs: vec![],
		};
		let epoch = req.epoch;

		// fetch validators for latest block
		let validator_state = ValidatorState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::ValidatorsFetchError(format!("Failed to initialize ValidatorState: {}", e))
		})?;
		let validators = validator_state
			.load_all_validators(epoch)
			.await
			.map_err(|e| NodeError::ValidatorsFetchError(format!("Failed to fetch validators for epoch {}: {}", epoch, e)))?
			.unwrap_or_else(Vec::new);
		let rpc_validators: Vec<l1x_rpc::rpc_model::Validator> = validators
			.iter()
			.map(|validator| {
				to_rpc_validator(validator).map_err(|e| NodeError::UnexpectedError(format!("Failed to convert validator: {}", e)))
			})
			.collect::<Result<Vec<_>, _>>()?;
		let validators_for_epoch = ValidatorsForEpoch {
			validators: rpc_validators,
		};

		response.validators_for_epochs.push(validators_for_epoch);

		if epoch > 0 {
			let previous_epoch = epoch - 1;
			let validators = validator_state
				.load_all_validators(previous_epoch)
				.await
				.map_err(|e| NodeError::ValidatorsFetchError(format!("Failed to fetch validators for epoch {}: {}", previous_epoch, e)))?
				.unwrap_or_else(Vec::new);

			let rpc_validators: Vec<l1x_rpc::rpc_model::Validator> = validators
				.iter()
				.map(|validator| {
					to_rpc_validator(validator).map_err(|e| NodeError::UnexpectedError(format!("Failed to convert validator: {}", e)))
				})
				.collect::<Result<Vec<_>, _>>()?;

			let validators_for_previous_epoch = ValidatorsForEpoch {
				validators: rpc_validators,
			};

			response.validators_for_epochs.push(validators_for_previous_epoch);
		}

		Ok(response)
	}

	pub async fn get_block_info(
		&self,
		req: GetBlockInfoRequest,
	) -> Result<GetBlockInfoResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;

		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;
		let vote_result_state = VoteResultState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access VoteResult database: {}", e))
		})?;
		let validator_state = ValidatorState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access Validator database: {}", e))
		})?;

		let block_number = parse_block_number(req.block_number).await?;
		let cluster_address = self.node.cluster_address;

		let block_header = block_state
			.load_block_header(block_number, &cluster_address)
			.await
			.map_err(|e| NodeError::BlockFetchError(format!("Failed to get block: {}", e)))?;

		let vote_result = vote_result_state
			.load_vote_result(&block_header.block_hash)
			.await
			.map_err(|e| NodeError::BlockFetchError(format!("Failed to get vote_result: {}", e)))?;

		let validators = validator_state
			.load_all_validators(block_header.epoch)
			.await
			.map_err(|e| NodeError::ValidatorsFetchError(format!("Failed to get validators for epoch: {}", e)))?
			.ok_or(NodeError::ValidatorsFetchError(format!("No validators selected for this epoch")))?;

		let validator_xscore_map: HashMap<Address, f64> = validators
			.into_iter()
			.map(|validator| (validator.address.clone(), validator.xscore))
			.collect();

		let mut validator_details: Vec<ValidatorDetail> = Vec::new();

		for vote in vote_result.data.votes {
			let validator_address = Account::address(&vote.verifying_key)
				.map_err(|e| NodeError::ParseError(format!("Unable to get Address from verifying_key: {}", e)))?;
			let validator_xscore = validator_xscore_map
				.get(&validator_address)
				.ok_or(NodeError::ValidatorsFetchError("Validator not found".to_string()))?;

			let validator_detail = ValidatorDetail {
				validator_vote: vote.data.vote,
				validator_address: hex::encode(validator_address),
				validator_xscore: *validator_xscore,
			};
			validator_details.push(validator_detail);
		}

		let block_info = BlockInfo {
			block_header: Some(to_rpc_block_header_v3(block_header)),
			validator_details,
		};

		let response = GetBlockInfoResponse {
			block_info: Some(block_info)
		};

		Ok(response)
	}

	pub async fn get_runtime_config(
		&self,
		_request: GetRuntimeConfigRequest
	) -> Result<GetRuntimeConfigResponse, NodeError> {
		let rt_config = runtime_config::RuntimeConfigCache::get()
			.await
			.map_err(|e| {
				NodeError::RuntimeConfigFetchError(format!("Failed to get RuntimeConfig: {}", e))
			})?;

		let rt_stake_info = runtime_config::RuntimeStakingInfoCache::get()
			.await
			.map_err(|e| {
				NodeError::RuntimeConfigFetchError(format!("Failed to get RuntimeStakingInfoCache: {}", e))
			})?;

		let rt_deny_config = runtime_config::RuntimeDenyConfigCache::get()
			.await
			.map_err(|e| {
				NodeError::RuntimeConfigFetchError(format!("Failed to get RuntimeDenyConfigCache: {}", e))
			})?;

		let response = GetRuntimeConfigResponse {
			runtime_config: Some(to_rpc_runtime_config(rt_config)),
			runtime_staking_info: Some(to_rpc_runtime_staking_info(rt_stake_info)),
			runtime_deny_config: Some(to_rpc_runtime_deny_config(rt_deny_config)),
		};

		Ok(response)
	}

	pub async fn get_block_with_details_by_number(
		&self,
		request: GetBlockWithDetailsByNumberRequest
	) -> Result<GetBlockWithDetailsByNumberResponse, NodeError> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;
		let block_number = parse_block_number(request.block_number).await?;
		let cluster_address = self.node.cluster_address;

		let persisted_block = block_state
			.load_block_response(block_number, &cluster_address)
			.await
			.map_err(|e| NodeError::BlockFetchError(format!("Failed to get block: {}", e)))?;

		let mut transactions: Vec<rpc_model::TransactionV3Response> = vec![];
		for txn in persisted_block.transactions.into_iter() {
			let txn = rpc_model::TransactionV3Response {
				transaction: Some(to_proto_transaction_v3(txn.transaction)?),
				from: txn.from.to_vec(),
				transaction_hash: txn.tx_hash.to_vec(),
				block_hash: txn.block_hash.to_vec(),
				block_number: i64::try_from(txn.block_number).unwrap_or(i64::MAX),
				fee_used: txn.fee_used.to_string(),
				timestamp: txn.timestamp as u64,
			};

			transactions.push(txn);
		}
		let block = BlockV3 {
			number: block_number.to_string(),
			hash: hex::encode(persisted_block.block_header.block_hash),
			parent_hash: hex::encode(persisted_block.block_header.parent_hash),
			timestamp: persisted_block.block_header.timestamp,
			transactions,
			block_type: persisted_block.block_header.block_type as i32,
			cluster_address: hex::encode(cluster_address),
			state_hash: hex::encode(persisted_block.block_header.state_hash),
			block_version: persisted_block.block_header.block_version.to_string(),
			epoch: persisted_block.block_header.epoch.to_string(),
		};

		let vote_result: Option<VoteResultShort> = if request.include_vote_result && block.block_type != BlockType::SystemBlock as i32 {
			let vote_result_state = VoteResultState::new(&db_pool_conn).await.map_err(|e| {
				NodeError::DBError(format!("Failed to access VoteResult database: {}", e))
			})?;
			let vote_result = vote_result_state
				.load_vote_result(&persisted_block.block_header.block_hash)
				.await
				.map_err(|e| NodeError::BlockFetchError(format!("Failed to get vote_result: {}", e)))?;
			let validator_address = Account::address(&vote_result.verifying_key)
				.map_err(|e| NodeError::ParseError(format!("Unable to get Address from verifying_key: {}", e)))?;
			Some(VoteResultShort {
				validator_address: validator_address.to_vec(),
				signature: vote_result.signature,
				verifying_key: vote_result.verifying_key,
				vote_passed: vote_result.data.vote_passed,
				votes: bincode::serialize(&vote_result.data.votes).unwrap(),
			})
		} else {
			None
		};

		let validators = if request.include_validators {
			let validator_state = ValidatorState::new(&db_pool_conn)
				.await
				.map_err(|e| NodeError::DBError(format!("Failed to access Validator database: {}", e)))?;
			let validators = validator_state
				.load_all_validators(persisted_block.block_header.epoch)
				.await
				.map_err(|e| NodeError::ValidatorsFetchError(format!("Failed to get validators for epoch: {}", e)))?
				.ok_or_else(|| NodeError::ValidatorsFetchError(format!("No validators selected for this epoch")))?;
			validators
				.into_iter()
				.map(|validator| ValidatorShort {
					address: validator.address.to_vec(),
					epoch: validator.epoch,
					xscore: validator.xscore,
				})
				.collect()
		} else {
			vec![]
		};

		let response = GetBlockWithDetailsByNumberResponse {
			block: Some(block),
			vote_result,
			validators,
		};

		Ok(response)
	}
}

async fn address_string_to_bytes(address_str: &str) -> Result<[u8; 20], NodeError> {
	// Check if the string is exactly 40 characters long (20 bytes in hexadecimal representation)
	if address_str.len() / 2 != 20 {
		return Err(NodeError::InvalidAddress(format!(
			"Invalid address length {}",
			address_str.len() / 2
		)))
	}

	// Convert the hexadecimal string into bytes
	let bytes: Result<Vec<u8>, _> = (0..20)
		.map(|i| {
			u8::from_str_radix(&address_str[i * 2..(i * 2) + 2], 16).map_err(|e| e.to_string())
		})
		.collect();

	// Convert the Vec<u8> into [u8; 20] array
	let res = bytes
		.map(|vec| {
			let mut result = [0u8; 20];
			result.copy_from_slice(&vec);
			result
		})
		.map_err(|e| {
			NodeError::InvalidAddress(format!("Failed to convert address string to bytes: {e}",))
		})?;
	Ok(res)
}

async fn parse_hash(hash: &str) -> Result<[u8; 32], NodeError> {
	if hash.len() != 64 {
		Err(NodeError::ParseError(format!("Invalid hash str length: {}", hash.len())))
	} else {
		let mut bytes = [0u8; 32];

		for (i, hex_pair) in hash.as_bytes().chunks(2).enumerate() {
			let hash_str = std::str::from_utf8(hex_pair)
				.map_err(|_| NodeError::ParseError(format!("Invalid hex format {}", hash)))?;

			bytes[i] = u8::from_str_radix(hash_str, 16)
				.map_err(|_| NodeError::ParseError(format!("Invalid hex format {}", hash)))?;
		}

		Ok(bytes)
	}
}
#[allow(dead_code)]
async fn parse_amount(amount: String) -> Result<Balance, NodeError> {
	amount
		.parse::<Balance>()
		.map_err(|_| NodeError::ParseError("Failed to parse amount".to_string()))
}

pub async fn parse_block_number<T: AsRef<str>>(block_number: T) -> Result<BlockNumber, NodeError> {
	block_number
		.as_ref()
		.parse::<BlockNumber>()
		.map_err(|_| NodeError::ParseError("Failed to parse block a mount".to_string()))
}

async fn parse_block_version<T: AsRef<str>>(block_version: T) -> Result<BlockVersion, NodeError> {
	block_version
		.as_ref()
		.parse::<BlockVersion>()
		.map_err(|_| NodeError::ParseError("Failed to parse block a mount".to_string()))
}

#[allow(dead_code)]
async fn parse_u128(value: &str) -> Option<u128> {
	match u128::from_str(value) {
		Ok(parsed) => Some(parsed),
		Err(err) => {
			error!("Failed to parse u128: {}", err);
			None
		},
	}
}
