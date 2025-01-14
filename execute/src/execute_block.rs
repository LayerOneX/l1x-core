use crate::{execute_read_only_call::smart_contract_read_only_call, execute_transaction::ExecuteTransaction};
use anyhow::{anyhow, Error};
use async_scoped::TokioScope;
use block::block_state::BlockState;
use db::{
	db::DbTxConn,
	postgres::postgres::{PgConnectionType, PostgresDBConn},
};
use diesel::{prelude::*, r2d2::{ConnectionManager, PooledConnection}, result::Error as DieselError};
use log::{error, info, warn};
use primitives::*;
use state::UpdatedState;
use std::sync::Arc;
use system::{account::Account, block::Block, block_header::BlockHeader, network::EventBroadcast, transaction::Transaction};
use tokio::sync::{broadcast, Mutex};
use system::transaction::{TransactionMetadata, TransactionResult};
use crate::execute_token::ExecuteToken;

macro_rules! continue_on_error {
	($expr:expr) => {
		match $expr {
			Ok(ret) => ret,
			Err(e) => {
				error!("{}", e);
				continue;
			}
		}
	};
}
pub struct ExecuteBlock {}

impl<'a> ExecuteBlock {
	async fn execute_block_inner(
		block: &Block,
		db_pool_conn: &'a DbTxConn<'a>,
		event_tx: broadcast::Sender<EventBroadcast>
	) -> Result<Vec<EventData>, Error> {
		let block_number = block.block_header.block_number;
		let cluster_address = block.block_header.cluster_address.clone();

		info!("Execute block #{}", block_number);

		let (db_request_tx, _cancel_on_drop) = state::updated_state_db::run_db_handler().await?;
		let (block_updated_state, events) = tokio::task::block_in_place(move || -> Result<_, Error> {
			let mut block_updated_state = UpdatedState::new(db_request_tx);
		
			// Sort transactions by nonce
			let mut transactions = block.transactions.clone();
			transactions.sort_by_key(|transaction| transaction.nonce);
			let mut events = vec![];
			for transaction in &transactions {
				let account_address = continue_on_error!(Account::address(&transaction.verifying_key));
				let transaction_hash = continue_on_error!(transaction.transaction_hash());

				info!("Execute transaction hash: {}", hex::encode(&transaction_hash));
				
				let mut tx_updated_state = block_updated_state.clone();
	
				// increment nonce
				continue_on_error!(tx_updated_state.increment_nonce(&account_address));
	
				// locking fee
				let fee_recipeint = compile_time_config::config::FEE_RECIPIENT_MASTER_ADDRESS;
				info!("Locking fee limit: {}", transaction.fee_limit);
				continue_on_error!(ExecuteToken::just_transfer_tokens(&account_address,
												   &fee_recipeint,
												   transaction.fee_limit,
												   &mut tx_updated_state));
				
				// Save the previous changes
				block_updated_state.merge(&tx_updated_state);
	
				match Self::execute_transaction(
					&account_address,
					transaction,
					&cluster_address,
					block.block_header.block_number,
					&block.block_header.block_hash,
					block.block_header.timestamp,
					&mut tx_updated_state,
					event_tx.clone(),
					false,
				) {
					Ok(tx_result) => {
						info!("Transaction passed, tx hash: {}, block: #{}", hex::encode(&transaction_hash), block_number);
						
						if let Some(event) = tx_result.event {
							events.push(event);
						}
						// The below code can rause errors. Need to save the intermidiate state before it because
						// the TX state is valid
						block_updated_state.merge(&tx_updated_state);

						// refund unused fee
						let unused_fee = continue_on_error!(transaction.fee_limit.checked_sub(tx_result.fee)
							.ok_or(anyhow!("Error calculating unused fee")));
						info!("Refunding unused fee: {}", unused_fee);
						continue_on_error!(ExecuteToken::just_transfer_tokens(
							&fee_recipeint,
							&account_address,
							unused_fee,
							&mut tx_updated_state,
						));
	
						block_updated_state.merge(&tx_updated_state);
	
						let metadata = TransactionMetadata{
							transaction_hash,
							fee: tx_result.fee,
							burnt_gas: tx_result.gas_burnt,
							is_successful: tx_result.is_success,
						};
						// Update transaction metadata
						continue_on_error!(block_updated_state.store_transaction_metadata(metadata));
					}
					Err(tx_result) => {
						// discard tx_updated_state
						drop(tx_updated_state);

						info!("Transaction failed, tx hash: {}, block: #{}, error: {:?}",
							hex::encode(&transaction_hash), block_number, tx_result.error);
	
						// refund unused fee
						let unused_fee = continue_on_error!(transaction.fee_limit.checked_sub(tx_result.fee)
							.ok_or(anyhow!("Error calculating unused fee")));
						info!("Refunding unused fee: {}", unused_fee);
						continue_on_error!(ExecuteToken::just_transfer_tokens(
							&fee_recipeint,
							&account_address,
							unused_fee,
							&mut block_updated_state,
						));
						
						let metadata = TransactionMetadata{
							transaction_hash,
							fee: tx_result.fee,
							burnt_gas: tx_result.gas_burnt,
							is_successful: tx_result.is_success,
						};
						// Update transaction metadata
						continue_on_error!(block_updated_state.store_transaction_metadata(metadata));
					}
				}
			}
			Ok((block_updated_state, events))
		})?;
		
		if let Err(e) = apply_system_contracts_changes(&block_updated_state, &block.block_header).await {
			// Only log the error. If this function fails, it means outdated "runtime_config" will be used.
			// If we return an error here then the block will be reverted and the blockchain will get stuck. That shouldn't happen. 
			error!("Couldn't apply system contracts changes, block #{block_number}, error: {e}")
		}

		if let Err(e) = block_updated_state.commit(db_pool_conn).await {
			error!("Could not commit new Block state, error: {}", e);
			return Err(anyhow!("Could not commit Block state, error: {}", e));
		}
		
		let block_state = BlockState::new(db_pool_conn).await?;
		block_state.set_block_executed(block_number, &cluster_address).await?;
		Ok(events)
	}

	async fn execute_system_block_inner(
		block: &Block,
		db_pool_conn: &'a DbTxConn<'a>,
		event_tx: broadcast::Sender<EventBroadcast>
	) -> Result<Vec<EventData>, Error> {
		let block_number = block.block_header.block_number;
		let cluster_address = block.block_header.cluster_address.clone();

		let (db_request_tx, _cancel_on_drop) = state::updated_state_db::run_db_handler().await?;
		let (block_updated_state, events) = tokio::task::block_in_place(move || -> Result<_, Error> {
			let mut block_updated_state = UpdatedState::new(db_request_tx);
		
			// Sort transactions by nonce
			let mut transactions = block.transactions.clone();
			transactions.sort_by_key(|transaction| transaction.nonce);
			let mut events = vec![];
			for transaction in &transactions {
				let account_address = continue_on_error!(Account::address(&transaction.verifying_key));
				let transaction_hash = continue_on_error!(transaction.transaction_hash());

				let mut tx_updated_state = block_updated_state.clone();

				// increment nonce
				continue_on_error!(tx_updated_state.increment_nonce(&account_address));
	
				// Save the previous changes
				block_updated_state.merge(&tx_updated_state);
	
				match Self::execute_transaction(
					&account_address,
					transaction,
					&cluster_address,
					block.block_header.block_number,
					&block.block_header.block_hash,
					block.block_header.timestamp,
					&mut tx_updated_state,
					event_tx.clone(),
					false,
				) {
					Ok(tx_result) => {
						if let Some(event) = tx_result.event {
							events.push(event);
						}
						// The below code can rause errors. Need to save the intermidiate state before it because
						// the TX state is valid
						block_updated_state.merge(&tx_updated_state);

						let metadata = TransactionMetadata{
							transaction_hash,
							fee: 0,
							burnt_gas: tx_result.gas_burnt,
							is_successful: tx_result.is_success,
						};
						// Update transaction metadata
						continue_on_error!(block_updated_state.store_transaction_metadata(metadata));
					}
					Err(tx_result) => {
						// discard tx_updated_state
						drop(tx_updated_state);

						info!("Transaction failed, tx hash: {}, block: #{}, error: {:?}",
							hex::encode(&transaction_hash), block_number, tx_result.error);
	
						let metadata = TransactionMetadata{
							transaction_hash,
							fee: 0,
							burnt_gas: tx_result.gas_burnt,
							is_successful: tx_result.is_success,
						};
						// Update transaction metadata
						continue_on_error!(block_updated_state.store_transaction_metadata(metadata));
					}
				}
			}
			Ok((block_updated_state, events))
		})?;

		apply_system_contracts_changes(&block_updated_state, &block.block_header).await?;

		if let Err(e) = block_updated_state.commit(db_pool_conn).await {
			error!("Could not commit new Block state, error: {}", e);
			return Err(anyhow!("Could not commit Block state, error: {}", e));
		}
		
		let block_state = BlockState::new(db_pool_conn).await?;
		block_state.set_block_executed(block_number, &cluster_address).await?;
		Ok(events)
	}

	pub async fn execute_block(
		block: &Block,
		event_tx: broadcast::Sender<EventBroadcast>,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<Vec<EventData>, Error> {
		let block_number = block.block_header.block_number;
		let cluster_address = block.block_header.cluster_address.clone();
		let block_state = BlockState::new(&db_pool_conn).await?;
		if block_state.is_block_executed(block_number, &cluster_address).await? {
			info!("Block #{} is already executed", block_number);
			return Ok(vec![])
		}

		let mut ret_events = Vec::new();
		match db_pool_conn {
			DbTxConn::POSTGRES(conn) => match conn {
				PostgresDBConn { conn, config } => match conn {
					PgConnectionType::TxConn(conn) => {
						let (tx, rx) = tokio::sync::oneshot::channel();
						let mut locked_conn = conn.lock().await;
						let event1 =
							match locked_conn.transaction::<_, DieselError, _>(|conn: &mut PgConnection| {
								let result = TokioScope::scope_and_block(|scope| {
									scope.spawn_blocking(move || {
										let p_conn = PostgresDBConn {
											conn: PgConnectionType::TxConn(Arc::new(Mutex::new(conn))),
											config: config.clone(),
										};
										let db_tx_conn = DbTxConn::POSTGRES(p_conn);
										tokio::runtime::Runtime::new()
											.expect("error creating pg execute block runtime")
											.block_on(async {
												let result = if block.is_system_block() {
													Self::execute_system_block_inner(
														block,
														&db_tx_conn,
														event_tx.clone(),
													)
													.await
												} else {
													Self::execute_block_inner(
														block,
														&db_tx_conn,
														event_tx.clone(),
													)
													.await
												};
												result
											})
									})
								});
								match result.1.into_iter().next() {
									Some(result) => match result {
										Ok(result) => {
											match result {
												Ok(events) => Ok(events),
												Err(e) => {
													let _ = tx.send(e).map_err(|e| {
														error!("execute_block tx send error: {e:?}")
													});
													Err(DieselError::RollbackTransaction)
												},
											}
										},
										Err(e) => {
											error!("execute_block error: {:?}", e);
											Err(DieselError::RollbackTransaction)
										},
									},
									None => {
										error!("execute_block error: no result");
										Err(DieselError::RollbackTransaction)
									},
								}
							}) {
								Ok(events) => events,
								Err(e) => {
									error!("Execute Block has been reverted, error: {:?}", e);
									let detailed_err = rx.await.unwrap_or_else(|e| anyhow!("recv error: {e:?}"));
									Err(anyhow!("Execute Block error: {e:?}, {detailed_err:?}"))?
								},
							};
						ret_events = event1;
					},
					PgConnectionType::PgConn(conn) => {
						let (tx, rx) = tokio::sync::oneshot::channel();
						let event1 =
							match conn.lock().await.transaction::<_, DieselError, _>(|conn: &mut PooledConnection<ConnectionManager<diesel::PgConnection>>| {
								let result = TokioScope::scope_and_block(|scope| {
									scope.spawn_blocking(move || {
										let p_conn = PostgresDBConn {
											conn: PgConnectionType::TxConn(Arc::new(Mutex::new(conn))),
											config: config.clone(),
										};
										let db_tx_conn = DbTxConn::POSTGRES(p_conn);
										tokio::runtime::Runtime::new()
											.expect("error creating pg execute block runtime")
											.block_on(async {
												let result = if block.is_system_block() {
													Self::execute_system_block_inner(
														block,
														&db_tx_conn,
														event_tx.clone(),
													)
													.await
												} else {
													Self::execute_block_inner(
														block,
														&db_tx_conn,
														event_tx.clone(),
													)
													.await
												};
												result
											})
									})
								});
								match result.1.into_iter().next() {
									Some(result) => match result {
										Ok(result) => {
											match result {
												Ok(events) => Ok(events),
												Err(e) => {
													let _ = tx.send(e).map_err(|e| {
														error!("execute_block tx send error: {e:?}")
													});
													Err(DieselError::RollbackTransaction)
												},
											}
										},
										Err(e) => {
											error!("execute_block error: {:?}", e);
											Err(DieselError::RollbackTransaction)
										},
									},
									None => {
										error!("execute_block error: no result");
										Err(DieselError::RollbackTransaction)
									},
								}
							}) {
								Ok(events) => events,
								Err(e) => {
									error!("Execute Block has been reverted, error: {:?}", e);
									let detailed_err = rx.await.unwrap_or_else(|e| anyhow!("recv error: {e:?}"));
									Err(anyhow!("Execute Block error: {e:?}, {detailed_err:?}"))?
								},
							};
						ret_events = event1;
					}
				}
			},
			DbTxConn::CASSANDRA(_session) => {
				Err(anyhow!("execute_block: Cassandra is not supported"))?
			},
			DbTxConn::ROCKSDB(_db_path) => {
				Err(anyhow!("execute_block: RocksDB is not supported"))?
			},
		}

		Ok(ret_events)
	}

	pub fn execute_transaction(
		account_address: &Address,
		transaction: &Transaction,
		cluster_address: &Address,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
		_is_simulation: bool,
	) -> Result<TransactionResult, TransactionResult> {
		let total_fee: Balance;
		let total_gas_burnt: Gas;
		let is_success;
		let event: Option<Vec<u8>>;

		let (result, event1, fee_charged, gas_burnt) = ExecuteTransaction::execute_transaction(
			&transaction,
			&account_address,
			&cluster_address,
			block_number,
			block_hash,
			block_timestamp,
			updated_state,
			event_tx.clone(),
		);
		event = event1;
		// Update total fee
		total_fee = fee_charged;
		// Update burnt gas
		total_gas_burnt = gas_burnt;
		match result {
			Ok(_) => {
				is_success = true;
				match &event {
					None => info!("execute_transaction no error"),
					Some(e) => info!(
						"execute_transaction no error: {}",
						hex::encode(e)
					),
				} 
			},
			Err(error) => return Err(TransactionResult { event, fee: total_fee, gas_burnt: total_gas_burnt, is_success: false, error: Some(error) }),
		}

		info!("execute_transaction: event: {:?}, fee_used: {}, gas burnt: {}", event.as_ref().and_then(|v| Some(hex::encode(v))), total_fee, total_gas_burnt);
		Ok(TransactionResult { event, fee: total_fee, gas_burnt: total_gas_burnt, is_success, error: None })
	}

	pub async fn execute_estimate_fee(
		transaction: &Transaction,
		cluster_address: &Address,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		_db_pool_conn: Arc<&'a DbTxConn<'a>>,
		event_tx: broadcast::Sender<EventBroadcast>,
		account_address: Address,
		fees: &mut Balance,
	) -> Result<(), Error> {
		*fees = 0; // Initialize fees
		let (tx, _cancel_on_drop) = state::updated_state_db::run_db_handler().await?;
		let fees_result = tokio::task::block_in_place(|| {
			let mut updated_state = UpdatedState::new(tx);
			ExecuteTransaction::execute_estimate_fee(
				&transaction,
				&account_address,
				&cluster_address,
				block_number,
				block_hash,
				block_timestamp,
				&mut updated_state,
				event_tx,
			)
		});

		match fees_result {
			Ok(fees_value) => {
				*fees = fees_value; // Update fees
			}
			Err(fees_value) => {
				info!("Rolling back simulated transaction");
				*fees = fees_value;
			}
		}
		Ok(())
	}			
}

async fn apply_system_contracts_changes(updated_state: &UpdatedState, block_header: &BlockHeader) -> Result<(), Error> {
	if updated_state.is_contract_state_updated(&system_contracts::CONFIG_CONTRACT_INSTANCE_ADDRESS) {
		info!("Try to refresh System Config cache");
		refresh_system_runtime_config(updated_state, block_header).await?
	}
	if updated_state.is_contract_state_updated(&system_contracts::DENYLIST_CONTRACT_INSTANCE_ADDRESS) {
		info!("Try to refresh System Deny Config cache");
		refresh_system_runtime_deny_config(updated_state, block_header).await?
	}
	if updated_state.is_contract_state_updated(&system_contracts::STAKING_CONTRACT_INSTANCE_ADDRESS) {
		info!("Try to refresh System Staking cache");
		refresh_system_staking_config(updated_state, block_header).await?
	}
	Ok(())
}

pub async fn refresh_system_runtime_config(updated_state: &UpdatedState, block_header: &BlockHeader) -> Result<(), Error> {
	let params = system_contracts::ConfigContractCallParams::config();
	let mut temp_updated_state = updated_state.clone();
	let result = smart_contract_read_only_call(
		&system_contracts::CONFIG_CONTRACT_INSTANCE_ADDRESS,
		&params.function_name,
		&params.args,
		crate::consts::READONLY_SYSTEM_CALL_GAS_LIMIT,
		&block_header.cluster_address,
		block_header.block_number,
		block_header.timestamp,
		&block_header.block_hash,
		&mut temp_updated_state,
		).await.map_err(|e| {
			error!("Could not read a new RuntimeConfig, error: {}", e);
			anyhow!("Could not read a new RuntimeConfig, error: {}", e)
		})?;
	if let Err(e) = runtime_config::RuntimeConfigCache::refresh(&result).await {
		error!("Could not update RuntimeConfig, error: {}", e);
		if runtime_config::RuntimeConfigCache::is_initialized().await {
			info!("Use the previous RuntimeConfig");
			return Ok(())
		}

		info!("Try to apply the previous RuntimeConfig");
		let params = system_contracts::ConfigContractCallParams::prev_config();
		let result = smart_contract_read_only_call(
			&system_contracts::CONFIG_CONTRACT_INSTANCE_ADDRESS,
			&params.function_name,
			&params.args,
			crate::consts::READONLY_SYSTEM_CALL_GAS_LIMIT,
			&block_header.cluster_address,
			block_header.block_number,
			block_header.timestamp,
			&block_header.block_hash,
			&mut temp_updated_state,
			).await.map_err(|e| {
				error!("Could not read the previous RuntimeConfig, error: {}", e);
				anyhow!("Could not read the previous RuntimeConfig, error: {}", e)
			})?;
		if let Err(e) = runtime_config::RuntimeConfigCache::refresh(&result).await {
			error!("Could not update the previous RuntimeConfig, error: {}", e);
			return Err(e)
		}
		warn!("The previous RuntimeConfig has been applied");
		return Ok(());
	}
	info!("Runtime Config has been updated, block #{}", block_header.block_number);
	Ok(())
}

pub async fn refresh_system_runtime_deny_config(updated_state: &UpdatedState, block_header: &BlockHeader) -> Result<(), Error> {
	let params = system_contracts::DenyListContractCallParams::get_rb_lists_borsh();
	let mut temp_updated_state = updated_state.clone();
	let result = smart_contract_read_only_call(
		&system_contracts::DENYLIST_CONTRACT_INSTANCE_ADDRESS,
		&params.function_name,
		&params.args,
		crate::consts::READONLY_SYSTEM_CALL_GAS_LIMIT,
		&block_header.cluster_address,
		block_header.block_number,
		block_header.timestamp,
		&block_header.block_hash,
		&mut temp_updated_state,
		).await.map_err(|e| {
			error!("Could not read a new RuntimeDenyConfig, error: {}", e);
			anyhow!("Could not read a new RuntimeDenyConfig, error: {}", e)
		})?;
	if let Err(e) = runtime_config::RuntimeDenyConfigCache::refresh(&result).await {
		error!("Could not update RuntimeDenyConfig, error: {}", e);
		if runtime_config::RuntimeDenyConfigCache::is_initialized().await {
			info!("Use the previous RuntimeDenyConfig");
			return Ok(())
		}

		return Err(e);
	}
	info!("Runtime Deny Config has been updated, block #{}", block_header.block_number);
	Ok(())
}

pub async fn refresh_system_staking_config(updated_state: &UpdatedState, block_header: &BlockHeader) -> Result<(), Error> {
	let params = system_contracts::StakingPoolCallParams::get_pool_info_for_all_nodes();
	let mut temp_updated_state = updated_state.clone();
	let result = smart_contract_read_only_call(
		&system_contracts::STAKING_CONTRACT_INSTANCE_ADDRESS,
		&params.function_name,
		&params.args,
		crate::consts::READONLY_SYSTEM_CALL_GAS_LIMIT,
		&block_header.cluster_address,
		block_header.block_number,
		block_header.timestamp,
		&block_header.block_hash,
		&mut temp_updated_state,
		).await.map_err(|e| {
			error!("Could not read RuntimeStakingInfo, error: {}", e);
			anyhow!("Could not read RuntimeStakingInfo, error: {}", e)
		})?;
	
	runtime_config::RuntimeStakingInfoCache::refresh(&result).await?;

	info!("Runtime StakingInfo has been updated, block #{}", block_header.block_number);
	
	Ok(())
}