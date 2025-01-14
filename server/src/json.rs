use crate::{
	service::FullNodeService,
	traits::{eth::EvmCompatibilityServer, l1x::FullNodeJsonServer},
};
use account::account_state::AccountState;
use anyhow::{self};
use block::block_state::BlockState;
use contract::contract_state::ContractState;
use contract_instance::contract_instance_state::ContractInstanceState;
use ethereum_types::{H160, H256, U256, U64};
use state::UpdatedState;
use types::eth::{
	block::{Block, BlockNumber, BlockTransactions, Header},
	bytes::Bytes,
	filter::{Filter, Kind, Params},
	log::Log,
	signers::{EvmEthCallRequest, TransactionRequest},
	sync::{SyncInfo, SyncStatus},
	transaction::{Transaction, TransactionReceipt},
};
use execute::consts::{ESTIMATE_FEE_LIMIT, ETH_CALL_FEE_LIMIT};

use event::event_state::EventState;

use jsonrpsee::{
	core::{async_trait, SubscriptionResult},
	server::Server,
	types::ErrorObjectOwned,
	Methods, PendingSubscriptionSink, SubscriptionMessage,
};
use l1x_rpc::rpc_model::{
	EstimateFeeRequest, EstimateFeeResponse, GetAccountStateRequest, GetAccountStateResponse, GetBlockByNumberRequest,
	GetBlockV3ByNumberResponse, GetChainStateRequest, GetChainStateResponse, GetCurrentNonceRequest, GetCurrentNonceResponse,
	GetEventsRequest, GetEventsResponse, GetLatestBlockHeadersRequest, GetLatestBlockHeadersResponseV3,
	GetLatestTransactionsRequest, GetLatestTransactionsResponse, GetStakeRequest, GetStakeResponse,
	GetTransactionReceiptRequest, GetTransactionReceiptResponse, GetTransactionV3ReceiptResponse,
	GetTransactionsByAccountRequest, GetTransactionsByAccountResponse, GetTransactionsV3ByAccountResponse,
	SmartContractReadOnlyCallRequest, SmartContractReadOnlyCallResponse, SubmitTransactionRequest,
	SubmitTransactionRequestV2, SubmitTransactionResponse, GetCurrentNodeInfoRequest, GetCurrentNodeInfoResponse,
	GetBlockInfoRequest, GetBlockInfoResponse, GetRuntimeConfigRequest, GetRuntimeConfigResponse, GetBlockWithDetailsByNumberRequest,
	GetBlockWithDetailsByNumberResponse,
};
use lazy_static::lazy_static;
use log::{debug, error, info};
use prost::alloc::string::String;
use secp256k1::
	ecdsa::Signature
;
use serde_json::{json, Value};
use util::convert::u256_to_balance;
use std::net::SocketAddr;
use system::{errors::NodeError, network::EventBroadcast, access, contract as Contract, transaction::TransactionType, transaction::Transaction as SystemTransaction};
use tokio::{
	sync::{broadcast, broadcast::error::RecvError, mpsc},
	task, time,
};
use tower_http::cors::{Any, CorsLayer};
use execute::execute_block::ExecuteBlock;
use std::sync::Arc;
use types::eth::signers::TransactionMessage;
use primitives::{Balance, Nonce, SignatureBytes, VerifyingKeyBytes};
use system::transaction::TransactionVersion;

const MAX_CONNECTIONS: u32 = 10_000u32;

#[macro_export]
macro_rules! error_object {
	($message:expr, $data:expr) => {
		ErrorObjectOwned::owned(-32603, $message, $data)
	};
	($message:expr) => {
		ErrorObjectOwned::owned(-32603, $message, None::<()>)
	};
}

#[cfg(test)]
mod mockable {

	use jsonrpsee::{DisconnectError, SubscriptionMessage};
	use mockall::automock;

	pub struct SubscriptionSink {}
	#[automock]
	impl SubscriptionSink {
		pub async fn send(&self, msg: SubscriptionMessage) -> Result<(), DisconnectError> {
			Ok(())
		}
	}
}

use db::db::Database;
#[cfg(not(test))]
use jsonrpsee::SubscriptionSink;
use tonic::codegen::http::Method;

lazy_static! {
	static ref DEFAULT_GAS_AMOUNT: U256 = U256::from(1_000_000);
}

// const RETRY_LIMIT: usize = 10; // maximum number of retries
// const TIMEOUT_DURATION: tokio::time::Duration = tokio::time::Duration::from_secs(5); // 10
// seconds timeout

async fn vec_to_bytes(data: Vec<u8>) -> Result<Bytes, ErrorObjectOwned> {
	// Convert Vec<u8> to Bytes
	Ok(Bytes::from(data))
}

async fn convert_ethereum_log(value: Value) -> Result<Log, serde_json::Error> {
	// Assuming `value` is a serde_json::Value representing a Log
	serde_json::from_value(value)
}

#[cfg(test)]
use mockable::MockSubscriptionSink as SubscriptionSink;
use node_crate::node::mempool_add_transaction;
use system::mempool::ResponseMempool;

#[allow(dead_code)]
pub struct FullNodeJsonImpl {
	service: FullNodeService,
	mempool_json_rx: mpsc::Receiver<ResponseMempool>,
}

#[async_trait]
impl EvmCompatibilityServer for FullNodeJsonImpl {
	async fn syncing(&self) -> Result<SyncStatus, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;
		let block_state = match BlockState::new(&db_pool_conn).await {
			Ok(state) => state,
			Err(e) => {
				log::error!("Failed to initialize block state because {:?}", e);
				return Err(error_object!("Failed to initialize block state"));
			},
		};
		let current_block =
			match block_state.load_chain_state(self.service.node.cluster_address).await {
				Ok(state) => state.block_number,
				Err(e) => {
					log::error!("Failed to load chain state because {:?}", e);
					return Err(error_object!("Failed to load chain state"));
				},
			};
		Ok(SyncStatus::Info(SyncInfo {
			starting_block: U256::zero(),
			current_block: U256::from(current_block),
			highest_block: U256::from(current_block),
			warp_chunks_amount: None,
			warp_chunks_processed: None,
		}))
	}

	async fn coinbase(&self) -> Result<H160, ErrorObjectOwned> {
		Ok(H160::from([0u8; 20]))
	}

	async fn get_storage_at(
		&self,
		address: H160,
		index: H256,
		_block: BlockNumber,
	) -> Result<H256, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let contract_instance_state = match ContractInstanceState::new(&db_pool_conn).await {
			Ok(result) => result,
			Err(e) => {
				return Err(error_object!(&format!(
					"Failed to start contract instance state because of {}",
					e
				)));
			},
		};
		let key: Vec<u8> = index.as_bytes().to_vec();
		let result = match contract_instance_state.get_state_key_value(&address.0, &key).await {
			Ok(Some(result)) => result,
			Ok(None) => {
				return Err(error_object!(&format!(
					"Failed to get state key value because of {}",
					"Key not found"
				)));
			},
			Err(e) => {
				// Handle the error case
				return Err(error_object!(&format!(
					"Failed to get state key value because of {}",
					e
				)));
			},
		};

		Ok(H256::from_slice(&result))
	}

	async fn balance(
		&self,
		address: H160,
		_block_number: Option<BlockNumber>,
	) -> Result<U256, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let account_state = match AccountState::new(&db_pool_conn).await {
			Ok(result) => result,
			Err(e) => {
				return Err(error_object!(&format!(
					"Failed to start account state because of {}",
					e
				)));
			},
		};
		let balance = match account_state.get_balance(&address.0).await {
			Ok(result) => result,
			Err(_) => 0,
		};
		Ok(U256::from(balance))
	}

	async fn max_priority_fee_per_gas(&self) -> Result<U256, ErrorObjectOwned> {
		// let block_state = match BlockState::new().await {
		// 	Ok(result) => result,
		// 	Err(e) => {
		// 		return Err(error_object!(&format!(
		// 			"Failed to start block state because of {}",
		// 			e
		// 		)));
		// 	},
		// };
		// // https://github.com/ethereum/go-ethereum/blob/master/eth/ethconfig/config.go#L44-L51
		// let at_percentile = 60;
		// let block_count = 20;
		// let index = (at_percentile * 2) as usize;
		// let highest = match
		// block_state.load_chain_state(self.service.node.cluster_address).await{ 	Ok(state) =>
		// state.block_number as u64, 	Err(e) => {
		// 		return Err(error_object!(&format!(
		// 			"Failed to load chain state because of {}",
		// 			e
		// 		)));
		// 	},
		// };
		// let lowest = highest.saturating_sub(block_count - 1);
		// // https://github.com/ethereum/go-ethereum/blob/master/eth/gasprice/gasprice.go#L149
		// let mut rewards = Vec::new();
		// if let Ok(fee_history_cache) = &self.fee_history_cache.lock() {
		// 	for n in lowest..highest + 1 {
		// 		if let Some(block) = fee_history_cache.get(&n) {
		// 			let reward = if let Some(r) = block.rewards.get(index) {
		// 				U256::from(*r)
		// 			} else {
		// 				U256::zero()
		// 			};
		// 			rewards.push(reward);
		// 		}
		// 	}
		// } else {
		// 	return Err(internal_err("Failed to read fee oracle cache."));
		// }
		// Ok(*rewards.iter().min().unwrap_or(&U256::zero()))
		Ok(U256::zero())
	}

	//Get code of address.
	async fn get_code(
		&self,
		address: H160,
		_block: Option<BlockNumber>,
	) -> Result<Bytes, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;
		let contract_state = ContractState::new(&db_pool_conn).await.map_err(|e| {
			error_object!(&format!("Failed to start contract state because of: {}", e))
		})?;
		let contract = contract_state
			.get_contract(&address.0)
			.await
			.map_err(|_| error_object!("Can't fetch the contract"))?;

		Ok(Bytes::from(contract.code))
	}

	// Assume necessary imports and context are present
	async fn call(
		&self,
		request: EvmEthCallRequest,
		_block: BlockNumber,
	) -> Result<Bytes, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;

		let to = request
			.to
			.ok_or_else(|| error_object!("No 'to' address found in the request"))?;

		info!("RPC: Call: to={:?}", to);

		let (from, nonce) = match request.from {
			Some(from) => {
				let account_state = AccountState::new(&db_pool_conn)
					.await
					.map_err(|e| error_object!(format!("Can't get account state. DBError: {}", e)))?;
				let nonce = account_state.get_nonce(&from.0)
					.await
					.map_err(|e| error_object!(format!("Can't get account nonce. DBError: {}", e)))?;

				(from.into(), nonce)
			},
			None => {
				(H160::zero(), Nonce::default())
			}
		};

		let (block_number, block_hash, block_timestamp) = {
			let block_state = BlockState::new(&db_pool_conn)
				.await
				.map_err(|e| error_object!(format!("Can't get block state. DBError: {}", e)))?;
			
			let block_head_header = block_state.block_head_header(self.service.node.cluster_address)
				.await
				.map_err(|e| error_object!(format!("Can't get block_head_header. DBError: {}", e)))?;

			(block_head_header.block_number, block_head_header.block_hash, block_head_header.timestamp)
		};

		let deposit = request.value.unwrap_or_default();
		let deposit = u256_to_balance(&deposit).ok_or_else(|| error_object!(format!("Can't convert U256 to Balance")))?;

		let transaction_type = TransactionType::SmartContractFunctionCall {
			contract_instance_address: to.into(),
			function: vec![],
			arguments: request.data.unwrap_or(Bytes::default()).into(),
			deposit
		};

		// nonce, transaction_type, ETH_CALL_FEE_LIMIT, signature, verifying_key
		let transaction = system::transaction::Transaction {
			version: TransactionVersion::default(),
			nonce: nonce + 1,
			transaction_type,
			fee_limit: ETH_CALL_FEE_LIMIT,
			signature: SignatureBytes::default(),
			verifying_key: VerifyingKeyBytes::default(),
			eth_original_transaction: None,
		};
		
		let (tx, _cancel_on_drop) = state::updated_state_db::run_db_handler().await
			.map_err(|e| error_object!(format!("DB: Can't run handler, error: {}", e)))?;
		let res = tokio::task::block_in_place(|| {
			let mut updated_state = UpdatedState::new(tx);
			let (event_tx, _) = broadcast::channel(1000);
			execute::execute_block::ExecuteBlock::execute_transaction(
					&from.0, 
					&transaction, 
					&self.service.node.cluster_address, 
					block_number, 
					&block_hash, 
					block_timestamp, 
					&mut updated_state,
					event_tx,
					true
				)
		});

		let res = res.map_err(|e| error_object!(format!("{}", hex::encode(&e.event.unwrap_or_default()))))?;
		Ok(res.event.unwrap_or_default().into())
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Vec<f64>,
	) -> Result<ethers::types::FeeHistory, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;
		let block_state = BlockState::new(&db_pool_conn)
			.await
			.map_err(|e| error_object!(&format!("Failed to start BlockState due to {e}")))?;
		let range_limit = U256::from(1024);
		let block_count =
			if block_count > range_limit { range_limit.as_usize() } else { block_count.as_usize() };
		let gas_used_ratio: Vec<f64> = std::iter::repeat_with(|| 0.1).take(block_count).collect();
		let reward_percent = match reward_percentiles {
			percentiles => {
				let rewards: Vec<Vec<U256>> = (0..block_count)
					.map(|_| percentiles.iter().map(|_| U256::from(1u8)).collect())
					.collect();
				Some(rewards)
			},
		};
		// Determine the range of blocks to analyze
		let end_block = match newest_block {
			BlockNumber::Earliest => 0,
			BlockNumber::Latest => {
				let chain_state = block_state
					.load_chain_state(self.service.node.cluster_address)
					.await
					.map_err(|e| error_object!(&format!("Failed to load block due to {e}")))?;
				chain_state.block_number as u64
			},
			BlockNumber::Num(x) => x,
			_ => return Err(error_object!("Unsupported block number")),
		};

		let start_block = end_block.saturating_sub(block_count as u64);
		let mut base_fee_per_gas: Vec<U256> = Vec::new();
		base_fee_per_gas.push(*DEFAULT_GAS_AMOUNT);
		let reward_view: Option<Vec<Vec<U256>>> = reward_percent.map(|rewards| {
			rewards.iter().map(|inner| inner.iter().map(|u256| *u256).collect()).collect()
		});
		let reward = match reward_view {
			Some(output) => output,
			None => return Err(error_object!("Unsupported reward data")),
		};
		let fee_history = ethers::types::FeeHistory {
			oldest_block: start_block.into(),
			base_fee_per_gas,
			gas_used_ratio,
			reward,
		};
		Ok(fee_history)
	}

	async fn get_logs(&self, filter: Value) -> Result<Vec<Log>, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;

		// Clone the filter to manipulate it
		let mut modified_filter = filter.clone();

		if let Some(topics_array) = modified_filter.get_mut("topics") {
			let mut new_topics = topics_array.take().as_array().unwrap_or(&vec![]).clone();
			new_topics.resize(4, Value::Null); // Resize to ensure it has 4 elements, filling with nulls if needed
			*topics_array = Value::Array(new_topics);
		}

		let filter: Filter = serde_json::from_value(modified_filter)
			.map_err(|e| NodeError::ParseError(format!("Failed to parse log filter: {}", e)))?;
		println!("FILTER: {filter:?}");

		let event_state = EventState::new(&db_pool_conn)
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to access database: {}", e)))?;

		// Get filtered logs
		let filtered_logs = event_state
			.get_filtered_events(filter)
			.await
			.map_err(|e| NodeError::EventFetchError(format!("Failed to get logs: {}", e)))?;

		Ok(filtered_logs)
	}

	// needs to stream:
	// {
	//     "removed":false,
	//     "logIndex":"0x0",
	//     "transactionIndex":"0x0",
	//     "transactionHash":"0x6f8524f68597146124ae43dfa4997cbdf63ba98ccd5b5114c0c502c49f244d0d",
	//     "blockHash":"0x7a8decff523ca5f0f79bd03b05d657715773760b963abd18dbf23065c3b42597",
	//     "blockNumber":"0x1544a6",
	//     "address":"0x537393a37a3be4a8129e6ca9dad8329ba6eef228",
	//     "data":"0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000d48656c6c6f2c20776f726c642100000000000000000000000000000000000000",
	//     "topics":["0x8da45d748eefefd09cc1491cd32086b6d6a0bd7063d08f05c94df9eb1404bd80","
	// 0x0000000000000000000000002cbd9e754f49a520497ae12f7f407301ddf96ee9"] }
	async fn subscribe(
		&self,
		pending: PendingSubscriptionSink,
		kind: Kind,
		params: Option<Params>,
	) -> SubscriptionResult {
		// TBD: fetch the historical logs

		#[cfg(not(test))]
		if kind == Kind::Logs {
			let sink = pending.accept().await?;
			subscribe_future_events(&self.service.node.node_evm_event_tx, sink, params, None)
				.await?;
		} else {
			pending.reject(error_object!("Block number exceed")).await;
		}

		Ok(())
	}

	async fn block_number(&self) -> Result<String, ErrorObjectOwned> {
		let chain_state_resp = self.service.get_chain_state(GetChainStateRequest {}).await?;
		let block_number: u128 = chain_state_resp
			.head_block_number
			.parse()
			.map_err(|_| error_object!("Invalid block number"))?;
		Ok(format!("0x{block_number:x}"))
	}

	async fn chain_id(&self) -> Result<Option<U64>, ErrorObjectOwned> {
		Ok(Some(U64::from(self.service.eth_chain_id)))
	}

	async fn net_version(&self) -> Result<Option<String>, ErrorObjectOwned> {
		Ok(Some(self.service.eth_chain_id.to_string()))
	}

	async fn estimate_gas(
		&self,
		eth_calls: TransactionRequest,
		_block_number: Option<BlockNumber>,
	) -> Result<U256, ErrorObjectOwned> {
		let tx_message: TransactionMessage = eth_calls.clone().into();
		let legacy_tx_message = match &tx_message {
			TransactionMessage::Legacy(msg) => msg,
			_ => return Err(error_object!("Invalid transaction type")),
		};
		
		info!("RPC: EstimateGas: action={:?}", legacy_tx_message.action);

		// convert value to primitive balance type
		let value: u128 = legacy_tx_message.value.try_into().map_err(|e| error_object!(format!("Failed to convert legacy TX balance: {e}")))?;
		// create a new transaction type
		let tx_type = match legacy_tx_message.action {
			ethereum::TransactionAction::Call(address) => TransactionType::SmartContractFunctionCall {
				contract_instance_address: address.0,
				function: Vec::new(),
				arguments: legacy_tx_message.input.clone(),
				deposit: value,
			},
			ethereum::TransactionAction::Create => TransactionType::SmartContractDeployment {
				access_type: access::AccessType::PUBLIC,
				contract_type: Contract::ContractType::EVM,
				contract_code: legacy_tx_message.input.clone(),
				deposit: value,
				salt: [0; 32].to_vec(),
			},
		};
		let sender = match eth_calls.from {
			Some(address) => address.0,
			None => return Err(error_object!("No sender found")),
		};
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;
		let transaction = SystemTransaction {
			version: TransactionVersion::default(),
			nonce: legacy_tx_message.nonce.as_u128().checked_add(1).ok_or_else(|| error_object!("Nonce calculation overflow"))?,
			transaction_type: tx_type,
			fee_limit: ESTIMATE_FEE_LIMIT,
			signature: vec![], // default value
			verifying_key: vec![], // default value
			eth_original_transaction: None, // default value
		};
		// latest block number is used at the moment
		let block_num_hex = self.block_number().await.map_err(|_| error_object!("Unable to get block number"))?;
		let block_number_value = u64::from_str_radix(&block_num_hex[2..], 16).map_err(|_| error_object!("Invalid hex block number"))?;
		let mut fees: Balance = 0;
		let block_state = BlockState::new(&db_pool_conn)
			.await
			.map_err(|e| error_object!(&format!("Failed to get block_state due to {e}")))?;
		let latest_block_header =  block_state.block_head_header(self.service.node.cluster_address)
			.await
			.map_err(|e| error_object!(&format!("Failed to get latest_block_header due to {e}")))?;
		if let Err(e) =  ExecuteBlock::execute_estimate_fee(
			&transaction,
			&self.service.node.cluster_address,
			block_number_value.into(),
			&latest_block_header.block_hash,
			latest_block_header.timestamp,
			Arc::new(&db_pool_conn),
			self.service.node.node_evm_event_tx.clone(),
			sender,
			&mut fees,
		).await {
			return Err(error_object!("Unable to get fee"));
		};
		let gas_station = execute::execute_fee::SystemFeeConfig::gas_station_from_system_config();
		// incrementing fee by 1 to prevent 0 fee charged case
		Ok(gas_station.buy_gas(fees + 1).into())
	}

	async fn gas_price(&self) -> Result<U256, ErrorObjectOwned> {
		let gas_station = execute::execute_fee::SystemFeeConfig::gas_station_from_system_config();
		Ok(gas_station.gas_price().into())
	}

	/// Retrieves a block by its number.
	///
	/// This function fetches a block based on the specified block number. The block number can be a
	/// specific number or a special value like the earliest or latest block. The function currently
	/// has a placeholder for optionally including transactions in the block, which is not
	/// implemented yet.
	///
	/// # Arguments
	///
	/// * `block_number`: `BlockNumber` - The number of the block to retrieve. Can be a specific
	///   number or a special value like `Earliest` or `Latest`.
	/// * `_include_tx`: `bool` - A placeholder parameter for optionally including transactions in
	///   the block. This feature is currently not implemented.
	///
	/// # Returns
	///
	/// * `Result<Option<Block<Transaction>>, ErrorObjectOwned>` - On success, returns an
	///   `Option<Block<Transaction>>` containing the block details if the block is found. Returns
	///   `None` if the block is not found. On failure, returns an `ErrorObjectOwned` indicating the
	///   cause of the error.
	///
	/// # Errors
	///
	/// * Returns an error if there is a failure in initializing the block state, loading the chain
	///   state, or fetching the block.
	/// * Returns an error if the provided block number is unsupported or if the block cannot be
	///   found.
	///
	/// # Example
	///
	/// ```no_run
	/// # // Mock struct and function for demonstration purposes
	/// # #[derive(Debug)] // Implementing the Debug trait
	/// # struct Block<T> { block_number: u64, transactions: Vec<T> } // Example block structure
	/// #[derive(Debug)]
	/// # struct Transaction { /* transaction details */ } // Example transaction structure
	/// # enum BlockNumber {
	/// #     Earliest, Latest,
	/// #     // ... possibly other variants ...
	/// # }
	/// # async fn get_block_by_number(block_number: BlockNumber, _include_tx: bool) -> Result<Option<Block<Transaction>>, String> {
	/// #     Ok(Some(Block { block_number: 1, transactions: vec![] })) // Mocked block details
	/// # }
	/// # fn main() {
	/// let block_number = BlockNumber::Latest; // Example block number
	/// let block = tokio::runtime::Runtime::new().unwrap().block_on(async {
	///     get_block_by_number(block_number, false).await // '_include_tx' is not functional yet
	/// });
	///
	/// match block {
	///     Ok(Some(block)) => println!("Block details: {:?}", block),
	///     Ok(None) => println!("Block not found"),
	///     Err(e) => println!("Error fetching block: {:?}", e),
	/// }
	/// # }
	/// ```
	///
	/// # Panics
	///
	/// This function does not panic under normal operation. However, it will panic if there's an
	/// issue with converting transaction details to the expected format.
	///
	/// # Safety
	///
	/// This function is generally safe to use but relies on correct implementation of external
	/// dependencies like `BlockState` and proper error handling of async operations.
	///
	/// # Notes
	///
	/// * This function is an async function and requires `.await` for execution.
	/// * The function currently has a placeholder for including transactions in the block, which is
	///   not implemented.
	async fn get_block_by_number_eth(
		&self,
		block_number: BlockNumber,
		include_tx: bool, // FIXME: optionally include the txn
	) -> Result<Option<Block>, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;
		let block_state = BlockState::new(&db_pool_conn)
			.await
			.map_err(|e| error_object!(&format!("Failed to start BlockState due to {e}")))?;
		let block_number = match block_number {
			BlockNumber::Earliest => 0,
			BlockNumber::Latest => {
				let chain_state = block_state
					.load_chain_state(self.service.node.cluster_address)
					.await
					.map_err(|e| error_object!(&format!("Failed to load block due to {e}")))?;
				chain_state.block_number
			},
			BlockNumber::Num(x) => x as u128,
			BlockNumber::Hash { hash, .. } => {
				block_state
					.get_block_number_by_hash(hash.into())
					.await
					.map_err(|e| error_object!(&format!("Failed to load block_number due to {e}")))?
			},
			_ => return Err(error_object!("Unsupported block number")),
		};

		let block = block_state
			.load_block(block_number, &self.service.node.cluster_address)
			.await
			.map_err(|_| error_object!("Failed to find block number", Some(block_number)))?;

		let transactions = if include_tx {
			// FIXME downcasting

			let txs: Result<Vec<Transaction>, String> = block
			.transactions
			.into_iter()
			.map(|t| {
					let from = system::account::Account::address(&t.verifying_key).map_err(|e| {
						format!("Can't get address from public key, error: {}", e)
					})?;
					let from: H160 = from.into();
					Ok(Transaction {
						nonce: t.nonce.into(),
						max_fee_per_gas: Some(t.fee_limit.into()), // FIXME: verify
						from,
						..Transaction::default()
					})
				})
				.collect();
			BlockTransactions::Full(txs.map_err(|e: String| error_object!(e))?)
		} else {
			let hashes: Result<Vec<H256>, _> = block
				.transactions
				.into_iter()
				.map(|t| {
					Ok(H256::from(
						t.transaction_hash()
							.map_err(|_| "Internal: can't get a transaction hash".to_string())?,
					))
				})
				.collect();
			BlockTransactions::Hashes(hashes.map_err(|e: String| error_object!(e))?)
		};
		let txn_block = Some(Block {
			header: Header {
				hash: Some(block.block_header.block_hash.into()),
				parent_hash: block.block_header.parent_hash.into(),
				gas_used: *DEFAULT_GAS_AMOUNT,
				timestamp: block.block_header.timestamp.into(),
				number: Some((block.block_header.block_number as u64).into()),
				..Header::default()
			},
			transactions,
			..Block::default()
		});
		Ok(txn_block)
	}

	async fn get_block_by_hash(
		&self,
		block_hash: H256,
		include_tx: bool, // FIXME: optionally include the txn
	) -> Result<Option<Block>, ErrorObjectOwned> {
		self.get_block_by_number_eth(
			BlockNumber::Hash { hash: block_hash, require_canonical: false },
			include_tx,
		)
		.await
	}

	/// Retrieves the number of transactions sent from a given address up to a specified block
	/// number.
	///
	/// This function counts the number of transactions sent from the specified address. It allows
	/// an optional block number parameter to specify up to which block the transactions should be
	/// counted. The block number can be a specific number or one of several special values
	/// indicating the earliest, latest, pending, safe, or finalized block.
	///
	/// # Arguments
	///
	/// * `address`: `H160` - The address for which the transaction count is to be retrieved.
	/// * `block_number`: `Option<BlockNumber>` - An optional parameter specifying the block number
	///   up to which transactions should be counted. If `None`, it counts for all blocks.
	///
	/// # Returns
	///
	/// * `Result<U256, ErrorObjectOwned>` - On success, returns the count of transactions as
	///   `U256`. On failure, returns an `ErrorObjectOwned` indicating the cause of the error.
	///
	/// # Errors
	///
	/// * Returns an error if there is a failure in initializing the block state or loading the
	///   chain state.
	/// * Returns an error if the provided block number is unsupported.
	/// * Returns an error if there's an issue fetching the transaction count for the given address.
	///
	/// # Example
	///
	/// ```no_run
	/// # use primitive_types::H160;
	/// # use ethers::types::U256;
	/// # // Mock enum and function for demonstration purposes
	/// # enum BlockNumber {
	/// #     Earliest, Latest,
	/// #     // ... possibly other variants ...
	/// # }
	/// # async fn get_transaction_count(address: H160, block_number: Option<BlockNumber>) -> Result<U256, String> {
	/// #     Ok(U256::from(10)) // Mocked transaction count
	/// # }
	/// # fn main() {
	/// // Assuming valid Ethereum address bytes for demonstration purposes
	/// let address_bytes = [0u8; 20]; // Replace with actual Ethereum address bytes
	/// let address = H160::from_slice(&address_bytes);
	/// let block_number = Some(BlockNumber::Latest); // Example block number
	/// let transaction_count = tokio::runtime::Runtime::new().unwrap().block_on(async {
	///     get_transaction_count(address, block_number).await
	/// });
	///
	/// match transaction_count {
	///     Ok(count) => println!("Number of transactions: {}", count),
	///     Err(e) => println!("Error getting transaction count: {:?}", e),
	/// }
	/// # }
	/// ```
	///
	/// # Safety
	///
	/// This function is generally safe to use but relies on correct implementation of external
	/// dependencies like `BlockState` and proper error handling of async operations.
	///
	/// # Notes
	///
	/// * This function is an async function and requires `.await` for execution.
	/// * The function caters to different scenarios based on the block number provided.
	async fn get_transaction_count(
		&self,
		address: H160,
		block_number_arg: Option<BlockNumber>,
	) -> Result<U256, ErrorObjectOwned> {

		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;
		let block_state = BlockState::new(&db_pool_conn)
			.await
			.map_err(|e| error_object!(&format!("Failed to start BlockState due to {e}")))?;

		let chain_state = block_state
			.load_chain_state(self.service.node.cluster_address)
			.await
			.map_err(|e| error_object!(&format!("Failed to load block due to {e}")))?;

		let block_number = if let Some(block_number) = block_number_arg {
			let block_number = match block_number {
				BlockNumber::Earliest => 0,
				BlockNumber::Latest |
				BlockNumber::Pending |
				BlockNumber::Safe |
				BlockNumber::Finalized => {
					chain_state.block_number
				},
				BlockNumber::Num(x) => x as u128,
				_x => return Err(error_object!("Unsupported block number")),
			};
			Some(block_number)
		} else {
			None
		};

		let block_state = BlockState::new(&db_pool_conn)
			.await
			.map_err(|e| error_object!(&format!("Failed to start BlockState due to {e}")))?;

		let account_state = AccountState::new(&db_pool_conn)
			.await
			.map_err(|e| error_object!(&format!("Failed to create account state {e}")))?;
		
		let account = account_state.get_account(&address.0)
			.await
			.map_err(|e| error_object!(&format!("Failed to fetch account {e}")))?;

		let txn_count = match account.account_type {
			// In case of a contract, getTransactionCount should return `nonce`
			//
			// Only contract's accounts use System type.
			// But EVM contracts used User type before it was fixed in
			// https://github.com/L1X-Foundation-Consensus/l1x-consensus/pull/941
			system::account::AccountType::System => {
				account.nonce
			},
			system::account::AccountType::User => {
				// Always return account nonce for all types for `latest_blocks_range` recent blocks.
				//
				// Probably it's incorrect but otherwise Metamask will not work for old accounts when
				// `nonce` is not equal to TX count. In case of new accounts (from v2.3.0), `nonce` is 
				// always equal to TX count.
				if let Some(block_number) = block_number {
					let latest_blocks_range = 5;
					if block_number < chain_state.block_number.saturating_sub(latest_blocks_range) {
						block_state.get_transaction_count(&address.into(), Some(block_number))
							.await
							.map_err(|e| error_object!(&format!("Failed to get Trasaction count to {e}")))?.into()
					} else {
						account.nonce
					}
				} else {
					account.nonce
				}

			},
		};
			
		Ok(txn_count.into())
	}

	async fn get_block_transaction_count_by_number(
		&self,
		block: BlockNumber,
	) -> Result<U256, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
			NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
		})?;
		let block_state = BlockState::new(&db_pool_conn)
			.await
			.map_err(|e| error_object!(&format!("Failed to start BlockState due to {e}")))?;

		let block_number = match block {
			BlockNumber::Earliest => 0,
			BlockNumber::Latest |
			BlockNumber::Pending |
			BlockNumber::Safe |
			BlockNumber::Finalized => {
				let chain_state = block_state
					.load_chain_state(self.service.node.cluster_address)
					.await
					.map_err(|e| error_object!(&format!("Failed to load block due to {e}")))?;
				chain_state.block_number
			},
			BlockNumber::Num(x) => x as u128,
			_x => return Err(error_object!("Unsupported block number")),
		};

		let block = block_state
			.load_block(block_number, &self.service.node.cluster_address)
			.await
			.unwrap();
		let txn_count = block.transactions.len();
		Ok(txn_count.into())
	}

	/// Retrieves a transaction by its hash.
	///
	/// This function searches for a transaction using its unique hash and returns the transaction's
	/// details if found. It covers various transaction types including native token transfers,
	/// smart contract deployments, function calls, and staking/unstaking transactions.
	///
	/// # Arguments
	///
	/// * `hash`: `H256` - The hash of the transaction to be retrieved.
	///
	/// # Returns
	///
	/// * `Result<Option<Transaction>, ErrorObjectOwned>` - On success, returns an
	///   `Option<Transaction>` containing the transaction details if the transaction is found.
	///   Returns `None` if the transaction is not found. On failure, returns an `ErrorObjectOwned`
	///   indicating the cause of the error.
	///
	/// # Errors
	///
	/// * Returns an error if there is a failure in initializing the block state or fetching the
	///   transaction.
	/// * Returns an error if the transaction hash, block hash, or other required details cannot be
	///   properly processed.
	/// * Returns an error if the transaction type does not provide necessary information like value
	///   or recipient address.
	///
	/// # Example
	///
	/// ```no_run
	/// # use ethers::types::H256;
	/// # #[derive(Debug)] // Implementing the Debug trait
	/// # struct Transaction { /* transaction details */ }
	/// # async fn transaction_by_hash(hash: H256) -> Result<Option<Transaction>, String> { Ok(Some(Transaction { /* details */ })) }
	/// # fn main() {
	/// // Assuming valid transaction hash bytes for demonstration purposes
	/// let transaction_hash_bytes = [0u8; 32]; // Replace with actual transaction hash bytes
	/// let transaction_hash = H256::from_slice(&transaction_hash_bytes);
	/// let transaction = tokio::runtime::Runtime::new().unwrap().block_on(async {
	///     transaction_by_hash(transaction_hash).await
	/// });
	///
	/// match transaction {
	///     Ok(Some(tx)) => println!("Transaction details: {:?}", tx),
	///     Ok(None) => println!("Transaction not found"),
	///     Err(e) => println!("Error fetching transaction: {:?}", e),
	/// }
	/// # }
	/// ```
	///
	/// # Panics
	///
	/// This function does not panic under normal operation.
	///
	/// # Safety
	///
	/// This function is safe to use as it does not involve any unsafe code blocks.
	///
	/// # Notes
	///
	/// * This function is an async function and requires `.await` for execution.
	/// * The function handles various types of transactions and their specific data requirements.
	async fn transaction_by_hash(
		&self,
		hash: H256,
	) -> Result<Option<Transaction>, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;
		let block_state = match BlockState::new(&db_pool_conn).await {
			Ok(state) => state,
			Err(e) =>
				return Err(error_object!(&format!(
					"Failed to initialize block state because {:?}",
					e
				))),
		};
		let hash_string = format!("{:x}", hash);
		// Convert Rust String to ::prost::alloc::string::String using into()
		let prost_string: ::prost::alloc::string::String = hash_string.into();
		let req = GetTransactionReceiptRequest { hash: prost_string.clone() };
		let tx_receipt = match self.service.get_transaction_v3_receipt(req).await {
			Ok(res) => res,
			Err(e) => return Ok(None),
		};
		let response = tx_receipt.transaction;
		let tx_response = match response {
			Some(output) => output,
			None => return Err(error_object!("No transaction data found")),
		};
		let from_hash_bytes = match vec_to_bytes(tx_response.from).await {
			Ok(bytes_data) => bytes_data,
			Err(e) => return Err(e),
		};
		let from = H160::from_slice(&from_hash_bytes.0);
		let txn = match tx_response.transaction {
			Some(tx) => tx,
			None => return Err(error_object!("No txn hash computed")),
		};
		let block_hash_bytes = match vec_to_bytes(tx_response.block_hash).await {
			Ok(bytes_data) => bytes_data,
			Err(e) => return Err(e),
		};
		let block_hash = H256::from_slice(&block_hash_bytes.0);
		let block = block_state
			.load_block(tx_response.block_number as u128, &self.service.node.cluster_address)
			.await
			.map_err(|e| error_object!(&format!("Failed to get block due to {e}")))?;
		let mut transaction_index = None;
		for (index, response) in block.transactions.iter().enumerate() {
			if let Ok(response_hash) = response.transaction_hash() {
				if response_hash == hash.0 {
					transaction_index = Some(U256::from(index as u64));
					break; // Break the loop once the transaction is found
				}
			}
		}
		let nonce = match U256::from_dec_str(&txn.nonce) {
			Ok(output) => output,
			Err(_) => return Err(error_object!("No txn hash computed")),
		};
		let max_fee = match U256::from_dec_str(&txn.fee_limit) {
			Ok(output) => output,
			Err(_) => return Err(error_object!("No txn hash computed")),
		};
		let gas = U256::from(*DEFAULT_GAS_AMOUNT);
		let gas_price = {
			let gas_station = execute::execute_fee::SystemFeeConfig::gas_station_from_system_config();
			Some(U256::from(gas_station.gas_price()))
		};
		let chain_id = match self.chain_id().await {
			Ok(id) => match id {
				Some(id) => id.as_u64(),
				None => return Err(error_object!("Bad type chain_id found")),
			},
			Err(_) => return Err(error_object!("No chain_id found")),
		};
		// Assuming you have a Signature instance
		let signature = match Signature::from_compact(txn.signature.as_slice()) {
			Ok(sig) => sig,
			Err(_) => return Err(error_object!("Couldn't convert signature bytes")),
		};
		let v = match txn.eth_original_transaction {
			Some(bytes) => get_v(bytes)?,
			None => return Err(error_object!("Transaction type not Legacy")),
		};
		let (value, to, input) = match txn.transaction {
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::NativeTokenTransfer(data)) => {
				let parsed_amount = U256::from_str_radix(&data.amount, 10).map_err(|e|
					error_object!("Failed to parse amount, {e}")
				)?;
				let to_address = match vec_to_bytes(data.address).await {
					Ok(bytes_data) => Some(H160::from_slice(&bytes_data.0)),
					Err(_) => None,
				};
				(Some(parsed_amount), to_address, None)
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::SmartContractDeployment(data)) => {
				let value = U256::from_str_radix(&data.deposit, 10).map_err(|e| {
					error_object!(&format!("Can't convert deposit to value tx receipt: {e}"))
				})?;
				(Some(value), None, Some(data.contract_code))
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::SmartContractInit(_data)) => {
				return Err(error_object!("Init has not supported in tx receipt"));
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::SmartContractFunctionCall(data)) => {
				let value = U256::from_str_radix(&data.deposit, 10).map_err(|e| {
					error_object!(&format!("Can't convert deposit to value tx receipt: {e}"))
				})?;
				let to_address = match vec_to_bytes(data.contract_instance_address).await {
					Ok(bytes_data) => Some(H160::from_slice(&bytes_data.0)),
					Err(_) => None,
				};
				(Some(value), to_address, Some(data.arguments))
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::Stake(_data)) => {
				return Err(error_object!("Stake not supported in tx receipt"));
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::Unstake(_data)) => {
				return Err(error_object!("Unstake not supported in tx receipt"));
			},
			None => return Err(error_object!("No value found")),
		};
		let input = match input {
			Some(input) => Bytes::from(input),
			None => return Err(error_object!("No input found")),
		};
		let value = value.unwrap_or(U256::zero());

		let transaction = Transaction {
			hash,
			nonce,
			block_hash: Some(block_hash),
			block_number: Some(U256::from(tx_response.block_number)),
			transaction_index,
			from,
			to,
			value,
			max_priority_fee_per_gas: Some(max_fee),
			max_fee_per_gas: Some(max_fee),
			gas_price,
			gas,
			input,
			v: Some(U256::from(v)),
			r: U256::from(&signature.serialize_compact()[0..32]),
			s: U256::from(&signature.serialize_compact()[32..64]),
			chain_id: Some(U64::from(chain_id)),
			..Transaction::default()
		};
		Ok(Some(transaction))
	}

	/// Retrieves the transaction receipt for a given transaction hash.
	///
	/// This function fetches the transaction receipt associated with the specified transaction
	/// hash. It includes various details such as the transaction index, block hash, gas used,
	/// contract address, and logs.
	///
	/// # Arguments
	///
	/// * `hash`: `H256` - The hash of the transaction for which the receipt is being requested.
	///
	/// # Returns
	///
	/// * `Result<Option<TransactionReceipt>, ErrorObjectOwned>` - On success, returns an
	///   `Option<TransactionReceipt>` containing the transaction receipt if found. If the
	///   transaction receipt is not found, it returns `None`. On failure, returns an
	///   `ErrorObjectOwned` indicating the cause of the error.
	///
	/// # Errors
	///
	/// * Returns an error if there is a failure in initializing the block state, fetching the
	///   transaction, or if the transaction receipt is not found.
	/// * Returns an error if the transaction hash, block hash, or other required details cannot be
	///   properly processed.
	/// * Returns an error if there is a failure in fetching events related to the transaction.
	///
	/// # Example
	///
	/// ```no_run
	/// # // Mock function for demonstration purposes
	/// # async fn transaction_receipt(hash: ethers::types::H256) -> Result<Option<String>, String> { Ok(Some("mock_receipt".to_string())) }
	/// # fn main() {
	/// use ethers::types::H256;
	/// // Assuming valid transaction hash bytes for demonstration purposes
	/// let transaction_hash_bytes = [0u8; 32]; // Replace with actual transaction hash bytes
	/// let transaction_hash = H256::from_slice(&transaction_hash_bytes);
	/// let receipt = tokio::runtime::Runtime::new().unwrap().block_on(async {
	///     transaction_receipt(transaction_hash).await
	/// });
	///
	/// match receipt {
	///     Ok(Some(receipt)) => println!("Transaction Receipt: {:?}", receipt),
	///     Ok(None) => println!("No receipt found for the transaction"),
	///     Err(e) => println!("Error fetching transaction receipt: {:?}", e),
	/// }
	/// # }
	/// ```
	///
	/// # Panics
	///
	/// This function does not panic under normal operation.
	///
	/// # Safety
	///
	/// This function is safe to use as it does not involve any unsafe code blocks.
	///
	/// # Notes
	///
	/// * This function is an async function and requires `.await` for execution.
	/// * The function covers various scenarios including native token transfers, smart contract
	///   deployments, function calls, and staking/unstaking transactions.
	async fn transaction_receipt(
		&self,
		hash: H256,
	) -> Result<Option<TransactionReceipt>, ErrorObjectOwned> {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| error_object!(&format!("Failed to get db_pool_conn: {}", e)))?;
		let block_state = match BlockState::new(&db_pool_conn).await {
			Ok(res) => res,
			Err(e) =>
				return Err(error_object!(&format!(
					"Failed to initialize block state because {:?}",
					e
				))),
		};
		let hash_string = format!("{:x}", hash);
		// Convert Rust String to ::prost::alloc::string::String using into()
		let prost_string: ::prost::alloc::string::String = hash_string.into();
		let req = GetTransactionReceiptRequest { hash: prost_string.clone() };
		let tx_receipt = match self.service.get_transaction_v3_receipt(req).await {
			Ok(res) => res,
			Err(_) => return Ok(None),
		};
		let tx_response = match tx_receipt.transaction {
			Some(output) => output,
			None => return Err(error_object!("No transaction data found")),
		};
		let tx_type = match tx_response.transaction {
			Some(tx) => tx,
			None => return Err(error_object!("No txn hash computed")),
		};
		let to = match tx_type.transaction {
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::NativeTokenTransfer(data)) => {
				let to_address = match vec_to_bytes(data.address).await {
					Ok(bytes_data) => Some(H160::from_slice(&bytes_data.0)),
					Err(_) => None,
				};
				to_address
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::SmartContractDeployment(data)) => {
				None
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::SmartContractInit(_data)) => {
				return Err(error_object!("Init has not supported in tx receipt"));
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::SmartContractFunctionCall(data)) => {
				let to_address = match vec_to_bytes(data.contract_instance_address).await {
					Ok(bytes_data) => Some(H160::from_slice(&bytes_data.0)),
					Err(_) => None,
				};
				to_address
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::Stake(_data)) => {
				return Err(error_object!("Stake not supported in tx receipt"));
			},
			Some(l1x_rpc::rpc_model::transaction_v3::Transaction::Unstake(_data)) => {
				return Err(error_object!("Unstake not supported in tx receipt"));
			},
			None => return Err(error_object!("No value found")),
		};
		let block_hash_bytes = match vec_to_bytes(tx_response.block_hash).await {
			Ok(bytes_data) => bytes_data,
			Err(e) => return Err(e),
		};
		let block_hash = H256::from_slice(&block_hash_bytes.0);
		let fees_used = match U256::from_dec_str(&tx_response.fee_used) {
			Ok(fee) => fee,
			Err(e) => return Err(error_object!(&format!("Fee used not got because: {:?}", e))),
		};
		let tx_hash_bytes = match vec_to_bytes(tx_response.transaction_hash).await {
			Ok(bytes_data) => bytes_data,
			Err(e) => return Err(e),
		};
		let block = block_state
			.load_block(tx_response.block_number as u128, &self.service.node.cluster_address)
			.await
			.map_err(|e| error_object!(&format!("Failed to get block due to {e}")))?;
		let mut transaction_index = None;
		for (index, response) in block.transactions.iter().enumerate() {
			if let Ok(response_hash) = response.transaction_hash() {
				if response_hash == hash.0 {
					transaction_index = Some(U256::from(index as u64));
					break; // Break the loop once the transaction is found
				}
			}
		}

		let transaction_hash = H256::from_slice(&tx_hash_bytes.0);
		let from_hash_bytes = match vec_to_bytes(tx_response.from).await {
			Ok(bytes_data) => bytes_data,
			Err(e) => return Err(e),
		};
		let from = H160::from_slice(&from_hash_bytes.0);
		let event_req = GetEventsRequest { tx_hash: prost_string, timestamp: 0 };
		let response = match self.service.get_events(event_req).await {
			Ok(res) => res,
			Err(e) => return Err(e.into()),
		};
		let result = match response.into_inner().recv().await {
			Some(Ok(res)) => res,
			Some(Err(e)) =>
				return Err(error_object!(&format!(
					"Failed to get send raw transaction response due to {e}"
				)))?,
			None => return Err(error_object!("No response found")),
		};
		let events: Vec<Vec<u8>> = result.events_data;
		let mut logs = Vec::new();
		for event in events {
			match serde_json::from_slice::<Value>(&event) {
				Ok(output) => {
					match convert_ethereum_log(output).await {
						Ok(log) => {
							let data = Log {
								address: log.address,
								topics: log.topics,
								data: log.data,
								block_hash: Some(block_hash),
								block_number: Some(tx_response.block_number.into()),
								transaction_hash: log.transaction_hash,
								transaction_index,
								log_index: log.log_index,
								transaction_log_index: log.transaction_log_index,
								logs_bloom: log.logs_bloom,
								..Log::default()
							};
							logs.push(data);
						},
						Err(e) => {
							log::error!("Error converting Ethereum log: {}", e);
							// Handle the error, e.g., continue to the next event, return an error,
							// etc.
							continue;
						},
					}
				},
				Err(e) => {
					log::error!("Error deserializing event: {}", e);
					// Handle the error, e.g., continue to the next event, return an error, etc.
					continue;
				},
			}
		}
		let mut address: Option<H160> = None;
		for log in logs.clone() {
			if log.transaction_hash == Some(hash) {
				address = Some(log.address)
			}
		}
		let status_code = if tx_receipt.status == 0 { 1 } else { 0 };
		let receipt = TransactionReceipt {
			transaction_hash: Some(transaction_hash),
			transaction_index,
			block_hash: Some(block_hash),
			block_number: Some((tx_response.block_number as u64).into()),
			from: Some(from),
			to,
			gas_used: Some(fees_used),
			contract_address: address,
			status_code: Some(status_code.into()),
			logs,
			..TransactionReceipt::default()
		};
		Ok(Some(receipt))
	}

	/// Sends a raw transaction to the network.
	///
	/// This function takes a raw transaction in the form of bytes, decodes it into an Ethereum
	/// TransactionV2, and sends it to the mempool for inclusion in a block. It supports only Legacy
	/// transactions.
	///
	/// # Arguments
	///
	/// * `bytes`: `Bytes` - A byte array representing the raw transaction data. This data should be
	///   in the format of an Ethereum transaction.
	///
	/// # Returns
	///
	/// * `Result<H256, ErrorObjectOwned>` - On success, returns the hash (`H256`) of the processed
	///   transaction. On failure, returns an `ErrorObjectOwned` indicating the cause of the error.
	///
	/// # Errors
	///
	/// * Returns an error if the input byte array is empty.
	/// * Returns an error if decoding the transaction fails, indicating a parsing issue.
	/// * Returns an error if the transaction type is not a Legacy transaction.
	/// * Returns an error if extracting the signature and public key from the transaction fails.
	/// * Returns an error if sending the transaction to the mempool fails.
	/// * Returns an error if the transaction is not included in a block.
	///
	/// # Example
	///
	/// ```no_run
	/// # // Mock function for demonstration purposes
	/// # async fn send_raw_transaction(bytes: &[u8]) -> Result<String, String> { Ok("mock_hash".to_string()) }
	/// # fn main() {
	/// use ethers::types::Bytes;
	/// // Assuming the transaction bytes are valid for the demonstration
	/// let transaction_bytes = "f86c018502540be40082520894095e7baea6a6c7c4c2dfeb977efac326af552d87038d7ea4c680008026a0a6b3a5c6f6f0e1b9b2c77e4e5c14c48e9e7f4e74ecdd8d7f2bb8ca2f5e5c6a07da0666fbddeea8accb1e6e7a1b7decd1a0734c0b2f57bca0cf3a9f6b461d4f1c50".as_bytes();
	/// let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
	///     send_raw_transaction(&transaction_bytes).await
	/// });
	///
	/// match result {
	///     Ok(hash) => println!("Transaction sent successfully. Hash: {:?}", hash),
	///     Err(e) => println!("Error sending transaction: {:?}", e),
	/// }
	/// # }
	/// ```
	///
	/// # Panics
	///
	/// This function does not panic under normal operation.
	///
	/// # Safety
	///
	/// This function is safe to use as it does not involve any unsafe code blocks.
	///
	/// # Notes
	///
	/// * This function is an async function and requires `.await` for execution.
	/// * The function is intended for use with the EVM, specifically handling Legacy transactions.
	async fn send_raw_transaction(&self, bytes: Bytes) -> Result<H256, ErrorObjectOwned> {
		let bytes = bytes.into_vec();
		// check if bytes are empty
		if bytes.is_empty() {
			return Err(error_object!("Transaction data is empty"));
		}
		let transaction = system::transaction::Transaction::new_from_eth(bytes)
			.map_err(|e| error_object!(&format!("Failed to convert transaction: {e}")))?;

		let sender =
			system::account::Account::address(&transaction.verifying_key).map_err(|e| {
				error_object!(&format!("Failed to get sender's address because of {}", e))
			})?;

		{
			// db_pool_conn should be destroyed as soon as possble
			let db_pool_conn = Database::get_pool_connection().await.map_err(|e| {
				NodeError::EventFetchError(format!("Failed to get db_pool_conn: {}", e))
			})?;

			validate::validate_common::ValidateCommon::validate_tx(
				&transaction,
				&sender,
				&db_pool_conn)
				.await
				.map_err(|e| {
					error_object!(&format!("Invalid transaction: {e}"))
				})?;

			// db_pool_conn and all related objects are destroyed here
		}
		// Sending the transaction to the mempool
		match mempool_add_transaction(self.service.node.mempool_tx.clone(), transaction.clone()).await {
			Ok(_) => {
				let hash = transaction.transaction_hash().map_err(|e| {
					error_object!(&format!("Failed to get transaction hash: {e}"))
				})?;
				Ok(hash.into())
			},
			Err(e) => Err(error_object!(&format!("{}", e)))
		}
	}
}

async fn subscribe_future_events(
	node_evm_event_tx: &broadcast::Sender<EventBroadcast>,
	sink: SubscriptionSink,
	params: Option<Params>,
	log_tx: Option<mpsc::Sender<Log>>, // for testing purposes
) -> anyhow::Result<()> {
	let (param_addresses, param_topics, from_block, to_block) = match params {
		Some(Params::Logs(filter)) => {
			let addresses = filter.address.to_opt_vec().unwrap_or(vec![]);
			// Topics described here: https://docs.ethers.org/v5/concepts/events/
			// Topic, supports `A` | `null` | `[A,B,C]` | `[A,[B,C]]` | `[null,[B,C]]` |
			// `[null,[null,C]]`
			let topics = filter
				.topics
				.into_iter()
				.map(|topics| match topics.unwrap().to_opt_vec() {
					Some(topics) if topics.contains(&None) => None,
					Some(topics) =>
						Some(topics.into_iter().map(|x| x.unwrap()).collect::<Vec<_>>()),
					None => None,
				})
				.collect::<Vec<_>>();
			(addresses, topics, filter.from_block, filter.to_block)
		},
		_ => (vec![], vec![], Some(BlockNumber::Num(u64::MIN)), Some(BlockNumber::Num(u64::MAX))),
	};
	// receive the latest events and stream them
	let mut evm_event_rx = node_evm_event_tx.subscribe();

	loop {
		match time::timeout(time::Duration::from_millis(100), async { evm_event_rx.recv() }).await {
			Ok(fut_res) => {
				match fut_res.await {
					Ok(EventBroadcast::Evm(
						address,
						topics,
						data,
						block_number,
						block_hash,
						txn_hash,
						txn_index,
						log_index,
					)) => {
						// entry criteria
						let prior_to_block = to_block.map_or(true, |to| to.gte(block_number));

						if !prior_to_block {
							debug!("not reached from block number, unsubscribing");
							break
						}

						// exit criteria
						let after_from_block =
							from_block.map_or(true, |from| from.lte(block_number));
						let address_match_criteria = param_addresses.contains(&address);
						let topic_match_criteria = topics
							.iter()
							.zip(param_topics.iter())
							.all(|(x, y)| y.as_ref().map_or(true, |z| z.contains(&x)));

						if after_from_block && address_match_criteria && topic_match_criteria {
							let log = Log {
								address,
								topics,
								data: data.into(),
								block_hash: Some(block_hash.into()),
								block_number: Some(
									TryInto::<u64>::try_into(block_number).unwrap().into(),
								),
								transaction_hash: Some(txn_hash.into()),
								transaction_index: Some(txn_index.into()),
								log_index: Some(log_index.into()),
								logs_bloom: None,
								transaction_log_index: None,
								removed: false,
							};

							let json_msg = &serde_json::to_value(&log)?;
							sink.send(SubscriptionMessage::from_json(json_msg)?).await?;

							// for testing purposes
							match log_tx {
								Some(ref log_tx) => {
									log_tx.send(log).await?;
								},
								_ => {},
							}
						}
					},
					Err(RecvError::Lagged(x)) => {
						debug!("receiver lagging {x}, continuing")
					},
					Err(RecvError::Closed) => {
						debug!("receiver closed, subscription revoked");
						break
					},
				}
			},
			Err(elapsed) => {
				debug!("elapsed, continuing {:?}", elapsed);
			},
		}
	}

	Ok(())
}

#[tonic::async_trait]
impl FullNodeJsonServer for FullNodeJsonImpl {
	async fn get_account_state(
		&self,
		request: GetAccountStateRequest,
	) -> Result<GetAccountStateResponse, ErrorObjectOwned> {
		Ok(self.service.get_account_state(request).await?)
	}

	async fn submit_transaction(
		&self,
		request: SubmitTransactionRequest,
	) -> Result<SubmitTransactionResponse, ErrorObjectOwned> {
		self.submit_transaction_v2(request.into()).await
	}

	async fn submit_transaction_v2(
		&self,
		request: SubmitTransactionRequestV2,
	) -> Result<SubmitTransactionResponse, ErrorObjectOwned> {
		let result = self.service.submit_transaction(request.into()).await.map_err(|e| {
			println!("ORIGINAL Error in submit transaction: {:?}", e);
			error_object!(&format!("Failed to get send raw transaction response due to {e}"))
		})?;
		// println!("SUBMIT TX result: {:?}", result.into_inner());
		let response = match result.into_inner().recv().await {
			Some(Ok(res)) => res,
			Some(Err(e)) =>
				return Err(error_object!(&format!(
					"Failed to get send raw transaction response due to {e}"
				)))?,
			None => return Err(error_object!("No response found")),
		};
		Ok(response)
	}

	async fn estimate_fee(
		&self,
		request: EstimateFeeRequest,
	) -> Result<EstimateFeeResponse, ErrorObjectOwned> {
		Ok(self.service.estimate_fee(request).await?)
	}

	async fn get_transaction_receipt(
		&self,
		request: GetTransactionReceiptRequest,
	) -> Result<GetTransactionReceiptResponse, ErrorObjectOwned> {
		Ok(self.service.get_transaction_receipt(request).await?)
	}

	async fn get_transaction_v3_receipt(
		&self,
		request: GetTransactionReceiptRequest,
	) -> Result<GetTransactionV3ReceiptResponse, ErrorObjectOwned> {
		Ok(self.service.get_transaction_v3_receipt(request).await?)
	}

	async fn get_transactions_by_account(
		&self,
		request: GetTransactionsByAccountRequest,
	) -> Result<GetTransactionsByAccountResponse, ErrorObjectOwned> {
		Ok(self.service.get_transactions_by_account(request).await?)
	}

	async fn get_transactions_v3_by_account(
		&self,
		request: GetTransactionsByAccountRequest,
	) -> Result<GetTransactionsV3ByAccountResponse, ErrorObjectOwned> {
		Ok(self.service.get_transactions_v3_by_account(request).await?)
	}

	async fn smart_contract_read_only_call(
		&self,
		request: SmartContractReadOnlyCallRequest,
	) -> Result<SmartContractReadOnlyCallResponse, ErrorObjectOwned> {
		Ok(self.service.smart_contract_read_only_call(request).await?)
	}

	async fn get_chain_state(
		&self,
		request: GetChainStateRequest,
	) -> Result<GetChainStateResponse, ErrorObjectOwned> {
		Ok(self.service.get_chain_state(request).await?)
	}

	async fn get_block_by_number(
		&self,
		request: GetBlockByNumberRequest,
	) -> Result<GetBlockV3ByNumberResponse, ErrorObjectOwned> {
		Ok(self.service.get_block_v3_by_number(request).await?)
	}

	async fn get_latest_block_headers(
		&self,
		pending: PendingSubscriptionSink,
		request: GetLatestBlockHeadersRequest,
	) -> SubscriptionResult {
		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to get db_pool_conn: {}", e)))?;
		let sink = pending.accept().await?;

		BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let num_headers: u32 = request.number_of_blocks;
		let headers_per_page: usize = request.blocks_per_page.try_into().unwrap_or(usize::MAX);

		let block_headers = self.service.get_latest_block_headers(num_headers).await?;

		let mut chunks = block_headers.chunks(headers_per_page);
		let mut page_number: u32 = 1;
		while let Some(chunk) = chunks.next().clone() {
			let headers = chunk.to_vec();
			let tx_response = GetLatestBlockHeadersResponseV3 { page_number, page: headers };
			let response = SubscriptionMessage::from_json(&tx_response)?;
			sink.send(response).await?;

			page_number += 1;
		}

		Ok(())
	}

	/// Receive the given number of latest transactions in a paged response format
	async fn get_latest_transactions(
		&self,
		pending: PendingSubscriptionSink,
		request: GetLatestTransactionsRequest,
	) -> SubscriptionResult {
		let sink = pending.accept().await?;

		let db_pool_conn = Database::get_pool_connection()
			.await
			.map_err(|e| NodeError::DBError(format!("Failed to get db_pool_conn: {}", e)))?;
		let block_state = BlockState::new(&db_pool_conn).await.map_err(|e| {
			NodeError::DBError(format!("Failed to access BlockState database: {}", e))
		})?;

		let num_transactions: u32 = request.number_of_transactions;
		let tx_per_page: usize = request.transactions_per_page.try_into().unwrap_or(usize::MAX);

		let transactions =
			block_state.load_latest_transactions(num_transactions).await.map_err(|e| {
				NodeError::BlockFetchError(format!("Failed to get latest transactions: {}", e))
			})?;

		let mut chunks = transactions.chunks(tx_per_page);
		let mut page_number: u32 = 1;
		while let Some(chunk) = chunks.next().clone() {
			let txs = chunk.to_vec();
			let tx_response = GetLatestTransactionsResponse { page_number, page: txs };
			let response = SubscriptionMessage::from_json(&tx_response)?;
			sink.send(response).await?;

			page_number += 1;
		}

		Ok(())
	}

	async fn get_stake(
		&self,
		request: GetStakeRequest,
	) -> Result<GetStakeResponse, ErrorObjectOwned> {
		Ok(self.service.get_stake(request).await?)
	}

	async fn get_current_nonce(
		&self,
		request: GetCurrentNonceRequest,
	) -> Result<GetCurrentNonceResponse, ErrorObjectOwned> {
		Ok(self.service.get_current_nonce(request).await?)
	}

	async fn get_events(
		&self,
		request: GetEventsRequest,
	) -> Result<GetEventsResponse, ErrorObjectOwned> {
		let response = match self.service.get_events(request).await {
			Ok(res) => res,
			Err(e) => return Err(e.into()),
		};
		let result = match response.into_inner().recv().await {
			Some(Ok(res)) => res,
			Some(Err(e)) =>
				return Err(error_object!(&format!(
					"Failed to get send raw transaction response due to {e}"
				)))?,
			None => return Err(error_object!("No response found")),
		};
		Ok(result)
	}

	async fn get_block_info(
		&self,
		request: GetBlockInfoRequest,
	) -> Result<GetBlockInfoResponse, ErrorObjectOwned> {
		Ok(self.service.get_block_info(request).await?)
	}

	/// Subscribes to events via jrpc
	///
	/// Usage:
	/// ```sh
	/// wscat -c ws://127.0.0.1:50051
	/// Connected (press CTRL+C to quit)
	/// > {"id":1,"jsonrpc":"2.0","method":"l1x_subscribeEvents","params":[]}
	/// < {"jsonrpc":"2.0","result":6058624328756215,"id":1}
	///
	/// ...send an event from cli dir:
	/// ../target/debug/cli --private-key $PRIV_KEY submit-txn --payload-file-path txn-payload/native_token_transfer.json
	///
	/// ...and see the event in the wscat
	/// < {"jsonrpc":"2.0","method":"l1x_subscribeEvents","params":{"subscription":6058624328756215,"result":...msg...}}
	/// ```
	async fn subscribe_events(
		&self,
		pending: PendingSubscriptionSink,
		_kind: Kind,
		_params: Option<Params>,
	) -> SubscriptionResult {
		let sink = pending.accept().await?;

		let mut node_event_tx = self.service.node.node_event_tx.subscribe();
		while let Ok(event) = node_event_tx.recv().await {
			let event_str = serde_json::from_slice::<Value>(&event)
				.unwrap_or_else(|_| json!({ "error": format!("Unparsable event {:?}", &event) }));
			let event = SubscriptionMessage::from_json(&event_str)?;
			sink.send(event).await?;
		}
		Ok(())
	}

	async fn get_current_node_info(
		&self,
		request: GetCurrentNodeInfoRequest
	) -> Result<GetCurrentNodeInfoResponse, ErrorObjectOwned> {
		Ok(self.service.get_current_node_info(request).await?)
	}

	async fn get_runtime_config(
		&self,
		request: GetRuntimeConfigRequest,
	) -> Result<GetRuntimeConfigResponse, ErrorObjectOwned> {
		Ok(self.service.get_runtime_config(request).await?)
	}

	async fn get_block_with_details_by_number(
		&self,
		request: GetBlockWithDetailsByNumberRequest,
	) -> Result<GetBlockWithDetailsByNumberResponse, ErrorObjectOwned> {
		Ok(self.service.get_block_with_details_by_number(request).await?)
	}
}

pub async fn run_server(
	address: String,
	service: FullNodeService,
	mempool_res_rx: mpsc::Receiver<ResponseMempool>,
) -> anyhow::Result<SocketAddr> {
	// Add a CORS middleware for handling HTTP requests.
	// This middleware does affect the response, including appropriate
	// headers to satisfy CORS. Because any origins are allowed, the
	// "Access-Control-Allow-Origin: *" header is appended to the response.
	let cors = CorsLayer::new()
		// Allow `POST` when accessing the resource
		.allow_methods([Method::POST, Method::GET])
		// Allow requests from any origin
		.allow_origin(Any)
		.allow_headers([tonic::codegen::http::header::CONTENT_TYPE]);
	let middleware = tower::ServiceBuilder::new().layer(cors);

	// The RPC exposes the access control for filtering and the middleware for
	// modifying requests / responses. These features are independent of one another
	// and can also be used separately.
	// In this example, we use both features.
	// TODO: Extract this to a builder function
	let server = Server::builder()
		.max_connections(MAX_CONNECTIONS)
		.set_middleware(middleware)
		.build(address.parse::<SocketAddr>()?)
		.await?;

	let addr = server.local_addr()?;

	info!("JSON RPC on {}", addr);

	let (mempool_json_evm_tx, mempool_json_evm_rx) = mpsc::channel(1000);
	let (mempool_json_tx, mempool_json_rx) = mpsc::channel(1000);

	let rpc_server_evm_impl =
		FullNodeJsonImpl { service: service.clone(), mempool_json_rx: mempool_json_evm_rx };
	let rpc_server_impl = FullNodeJsonImpl { service, mempool_json_rx };
	task::spawn(mempool_response(mempool_res_rx, mempool_json_evm_tx, mempool_json_tx));
	let mut methods = Methods::new();
	let eth_methods: Methods = EvmCompatibilityServer::into_rpc(rpc_server_evm_impl).into();
	let l1x_methods: Methods = FullNodeJsonServer::into_rpc(rpc_server_impl).into();

	methods.merge(eth_methods)?;
	methods.merge(l1x_methods)?;

	let handle = server.start(methods);

	// Runs forever!
	let _ = tokio::spawn(handle.stopped()).await;

	Ok(addr)
}

pub async fn mempool_response(
	mut mempool_rx: mpsc::Receiver<ResponseMempool>,
	mempool_json_evm_tx: mpsc::Sender<ResponseMempool>,
	mempool_json_tx: mpsc::Sender<ResponseMempool>,
) {
	while let Some(mempool_response) = mempool_rx.recv().await {
		if let Err(e) = mempool_json_evm_tx.send(mempool_response.clone()).await {
			error!("Unable to write mempool_response to mempool_grpc_tx channel: {:?}", e);
		}
		if let Err(e) = mempool_json_tx.send(mempool_response.clone()).await {
			error!("Unable to write mempool_response to mempool_json_tx channel: {:?}", e);
		}
	}
}

pub fn get_v(bytes: Vec<u8>) -> Result<u8, ErrorObjectOwned> {
// decode bytes into ethereum TransactionV2
	let transaction: ethereum::TransactionV2 = ethereum::EnvelopedDecodable::decode(&bytes)
		.map_err(|e| error_object!("Failed to parse log ethereum Transaction v2: {e:?}"))?;
	if let ethereum::TransactionV2::Legacy(legacy_txn) = &transaction {
		let v = legacy_txn.signature.standard_v();
		Ok(if v > 26 { v - 27 } else { v })
	} else {
		Err(error_object!("Transaction type not Legacy"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use ethereum_types::{H160, H256};
	use tokio::sync::{broadcast, mpsc};
	use types::eth::filter::VariadicValue;

	async fn validate_events(
		params: Option<Params>,
		sent_events: Vec<EventBroadcast>,
		exp_logs: Vec<String>,
	) {
		let mut subscription_sink = SubscriptionSink::new();
		subscription_sink
			.expect_send()
			.times(exp_logs.len())
			.returning(move |msg| Ok(()));

		let (node_evm_event_tx, _) = broadcast::channel(32);
		let (log_tx, mut log_rx) = mpsc::channel(32);

		let node_evm_event_tx_clone = node_evm_event_tx.clone();
		let sub_handle = tokio::spawn(async move {
			subscribe_future_events(
				&node_evm_event_tx_clone,
				subscription_sink,
				params,
				Some(log_tx),
			)
			.await
			.unwrap();
		});

		tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

		// send the events
		for ev in sent_events {
			node_evm_event_tx.send(ev).unwrap();
		}

		tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

		sub_handle.abort();

		let mut collected_logs = Vec::new();
		while let Some(log) = log_rx.recv().await {
			collected_logs.push(format!("{}", log.data));
		}
		assert_eq!(exp_logs, collected_logs);
	}

	#[tokio::test]
	async fn test_events_block_number_restraints() -> Result<()> {
		let contract_address = "0x537393a37a3be4a8129e6ca9dad8329ba6eef228".parse::<H160>()?;
		validate_events(
			Some(Params::Logs(Filter {
				address: VariadicValue::Single(contract_address.clone()),
				topics: [
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
				],
				from_block: Some(BlockNumber::Num(2)),
				to_block: Some(BlockNumber::Num(5)),
				block_hash: None,
			})),
			vec![
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![],                   // topics
					vec![1, 1, 1],            // data
					1,                        // block_number - too early
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![],                   // topics
					vec![1, 2, 3],            // data
					2,                        // block_number
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![],                   // topics
					vec![4, 5, 6],            // data
					5,                        // block_number
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![],                   // topics
					vec![9, 9, 9],            // data
					6,                        // block_number - too late
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
			],
			vec!["0x010203".to_owned(), "0x040506".to_owned()],
		)
		.await;
		Ok(())
	}

	#[tokio::test]
	async fn test_events_block_number_restraints_2() -> Result<()> {
		let contract_address = "0x537393a37a3be4a8129e6ca9dad8329ba6eef228".parse::<H160>()?;
		validate_events(
			Some(Params::Logs(Filter {
				address: VariadicValue::Single(contract_address.clone()),
				topics: [
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
				],
				from_block: Some(BlockNumber::Earliest),
				to_block: Some(BlockNumber::Latest),
				block_hash: None,
			})),
			vec![
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![],                   // topics
					vec![1, 2, 3],            // data
					2,                        // block_number
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![],                   // topics
					vec![4, 5, 6],            // data
					5,                        // block_number
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
			],
			vec!["0x010203".to_owned(), "0x040506".to_owned()],
		)
		.await;
		Ok(())
	}

	#[tokio::test]
	async fn test_events_block_number_restraints_3() -> Result<()> {
		let contract_address = "0x537393a37a3be4a8129e6ca9dad8329ba6eef228".parse::<H160>()?;
		validate_events(
			Some(Params::Logs(Filter {
				address: VariadicValue::Single(contract_address.clone()),
				topics: [
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
				],
				from_block: None,
				to_block: None,
				block_hash: None,
			})),
			vec![
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![],                   // topics
					vec![1, 2, 3],            // data
					2,                        // block_number
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![],                   // topics
					vec![4, 5, 6],            // data
					5,                        // block_number
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
			],
			vec!["0x010203".to_owned(), "0x040506".to_owned()],
		)
		.await;
		Ok(())
	}

	#[tokio::test]
	async fn test_events_topic_restraints() -> Result<()> {
		let contract_address = "0x537393a37a3be4a8129e6ca9dad8329ba6eef228".parse::<H160>()?;
		let event_signature =
			"0x537393a37a3be4a8129e6ca9dad8329ba6eef228537393a37a3be4a8129e6ca9".parse::<H256>()?;
		let bogus =
			"0x1111111111111111111111111111111111111111111111111111111111111111".parse::<H256>()?;
		let field1 =
			"0x0000000000000000000000000000000000000000000000000000000000000001".parse::<H256>()?;
		let field2_1 =
			"0x0000000000000000000000000000000000000000000000000000000000000021".parse::<H256>()?;
		let field2_2 =
			"0x0000000000000000000000000000000000000000000000000000000000000022".parse::<H256>()?;
		let field3_1 =
			"0x0000000000000000000000000000000000000000000000000000000000000031".parse::<H256>()?;
		let field3_2 =
			"0x0000000000000000000000000000000000000000000000000000000000000032".parse::<H256>()?;

		validate_events(
			Some(Params::Logs(Filter {
				address: VariadicValue::Single(contract_address.clone()),
				topics: [
					Some(VariadicValue::Single(Some(event_signature))),
					Some(VariadicValue::Single(Some(field1))),
					Some(VariadicValue::Null),
					Some(VariadicValue::Multiple(vec![Some(field3_1), Some(field3_2)])),
				],
				from_block: None,
				to_block: None,
				block_hash: None,
			})),
			vec![
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![event_signature],    // topics
					vec![1, 2, 3],            // data
					2,                        // block_number
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(), // address
					vec![bogus],              // topics
					vec![2, 3, 4],            // data
					5,                        // block_number
					[0; 32],                  // block_hash
					[0; 32],                  // txn_hash
					0,                        // txn_index
					0,                        // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(),                          // address
					vec![event_signature, field1, field2_1, field3_1], // topics
					vec![3, 4, 5],                                     // data
					5,                                                 // block_number
					[0; 32],                                           // block_hash
					[0; 32],                                           // txn_hash
					0,                                                 // txn_index
					0,                                                 // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(),                          // address
					vec![event_signature, field1, field2_2, field3_2], // topics
					vec![4, 5, 6],                                     // data
					5,                                                 // block_number
					[0; 32],                                           // block_hash
					[0; 32],                                           // txn_hash
					0,                                                 // txn_index
					0,                                                 // log_index
				),
				EventBroadcast::Evm(
					contract_address.clone(),                         // address
					vec![event_signature, bogus, field2_2, field3_2], // topics
					vec![4, 5, 6],                                    // data
					5,                                                // block_number
					[0; 32],                                          // block_hash
					[0; 32],                                          // txn_hash
					0,                                                // txn_index
					0,                                                // log_index
				),
			],
			vec!["0x010203".to_owned(), "0x030405".to_owned(), "0x040506".to_owned()],
		)
		.await;
		Ok(())
	}

	#[tokio::test]
	async fn test_events_invalid_address() -> Result<()> {
		let contract_address = "0x537393a37a3be4a8129e6ca9dad8329ba6eef228".parse::<H160>()?;
		validate_events(
			Some(Params::Logs(Filter {
				address: VariadicValue::Single(contract_address.clone()),
				topics: [
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
					Some(VariadicValue::Single(None)),
				],
				from_block: None,
				to_block: None,
				block_hash: None,
			})),
			vec![EventBroadcast::Evm(
				H160::zero(), // address
				vec![],       // topics
				vec![],       // data
				0,            // block_number
				[0; 32],      // block_hash
				[0; 32],      // txn_hash
				0,            // txn_index
				0,            // log_index
			)],
			vec![],
		)
		.await;
		Ok(())
	}
}
