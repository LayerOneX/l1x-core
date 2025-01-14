use crate::{state_cas::StateCas, state_pg::StatePg, state_rock::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{base::BaseState, block::BlockState as BlockStateInternal};

use l1x_rpc::rpc_model::{self, TransactionResponse};
use primitives::*;
use rocksdb::DB;
use std::sync::Arc;
use system::{
	block::{Block, BlockResponse},
	block_header::BlockHeader,
	chain_state::ChainState,
	transaction::Transaction,
	transaction_receipt::TransactionReceiptResponse,
};
use system::transaction::TransactionMetadata;

/// Enum representing different internal implementations of state storage.
pub enum StateInternalImpl<'a> {
	StateRock(StateRock),
	StatePg(StatePg<'a>),
	StateCas(StateCas),
}

/// Structure representing the block state with an internal storage state.
pub struct BlockState<'a> {
	/// The internal storage state.
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> BlockState<'a> {
	/// Creates a new instance of `BlockState`.
	///
	/// # Arguments
	///
	/// * `db_pool_conn` - Database transaction connection.
	///
	/// # Returns
	///
	/// A `Result` containing the new `BlockState` if successful, otherwise an `Error`.
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a> = match &db_pool_conn {
			DbTxConn::POSTGRES(pg) => StateInternalImpl::StatePg(StatePg { pg }),
			DbTxConn::CASSANDRA(session) =>
				StateInternalImpl::StateCas(StateCas { session: session.clone() }),
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/block", db_path);
				StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				})
			},
		};

		let state = BlockState { state: Arc::new(state) };

		state.create_table().await?;
		Ok(state)
	}

	/// Creates the necessary table in the database for storing block data.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn create_table(&self) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create_table().await,
			StateInternalImpl::StatePg(s) => s.create_table().await,
			StateInternalImpl::StateCas(s) => s.create_table().await,
		}
	}

	/// Executes a raw query on the database.
	///
	/// # Arguments
	///
	/// * `query` - The query to execute.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.raw_query(query).await,
			StateInternalImpl::StatePg(s) => s.raw_query(query).await,
			StateInternalImpl::StateCas(s) => s.raw_query(query).await,
		}
	}

	/// Stores the block head information in the database.
	///
	/// # Arguments
	///
	/// * `cluster_address` - The address of the cluster.
	/// * `block_number` - The block number.
	/// * `block_hash` - The hash of the block.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn store_block_head(
		&self,
		cluster_address: &Address,
		block_number: BlockNumber,
		block_hash: BlockHash,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_block_head(cluster_address, block_number, block_hash).await,
			StateInternalImpl::StatePg(s) =>
				s.store_block_head(cluster_address, block_number, block_hash).await,
			StateInternalImpl::StateCas(s) =>
				s.store_block_head(cluster_address, block_number, block_hash).await,
		}
	}

	/// Stores multiple block headers in batch mode.
	///
	/// # Arguments
	///
	/// * `block_headers` - A vector of block headers to store.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn batch_store_block_headers(
		&self,
		block_headers: Vec<BlockHeader>,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.batch_store_block_headers(block_headers).await,
			StateInternalImpl::StatePg(s) => s.batch_store_block_headers(block_headers).await,
			StateInternalImpl::StateCas(s) => s.batch_store_block_headers(block_headers).await,
		}
	}

	/// Updates the block head information.
	///
	/// # Arguments
	///
	/// * `cluster_address` - The address of the cluster.
	/// * `big_block_number` - The big block number.
	/// * `block_hash` - The hash of the block.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn update_block_head(
		&self,
		cluster_address: Address,
		big_block_number: BlockNumber,
		block_hash: BlockHash,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.update_block_head(cluster_address, big_block_number, block_hash).await,
			StateInternalImpl::StatePg(s) =>
				s.update_block_head(cluster_address, big_block_number, block_hash).await,
			StateInternalImpl::StateCas(s) =>
				s.update_block_head(cluster_address, big_block_number, block_hash).await,
		}
	}

	/// Loads the chain state for the given cluster address.
	///
	/// # Arguments
	///
	/// * `cluster_address` - The address of the cluster.
	///
	/// # Returns
	///
	/// A `Result` containing the chain state if successful, otherwise an `Error`.
	pub async fn load_chain_state(&self, cluster_address: Address) -> Result<ChainState, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_chain_state(cluster_address).await,
			StateInternalImpl::StatePg(s) => s.load_chain_state(cluster_address).await,
			StateInternalImpl::StateCas(s) => s.load_chain_state(cluster_address).await,
		}
	}

	/// Retrieves the block associated with the given cluster address.
	///
	/// # Arguments
	///
	/// * `cluster_address` - The address of the cluster.
	///
	/// # Returns
	///
	/// A `Result` containing the block if successful, otherwise an `Error`.
	pub async fn block_head(&self, cluster_address: Address) -> Result<Block, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.block_head(cluster_address).await,
			StateInternalImpl::StatePg(s) => s.block_head(cluster_address).await,
			StateInternalImpl::StateCas(s) => s.block_head(cluster_address).await,
		}
	}

	/// Retrieves the block header associated with the given cluster address.
	///
	/// # Arguments
	///
	/// * `cluster_address` - The address of the cluster.
	///
	/// # Returns
	///
	/// A `Result` containing the block header if successful, otherwise an `Error`.
	pub async fn block_head_header(&self, cluster_address: Address) -> Result<BlockHeader, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.block_head_header(cluster_address).await,
			StateInternalImpl::StatePg(s) => s.block_head_header(cluster_address).await,
			StateInternalImpl::StateCas(s) => s.block_head_header(cluster_address).await,
		}
	}

	/// Stores a block in the database.
	///
	/// # Arguments
	///
	/// * `block` - The block to store.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn store_block(&self, block: Block) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.store_block(block).await,
			StateInternalImpl::StatePg(s) => s.store_block(block).await,
			StateInternalImpl::StateCas(s) => s.store_block(block).await,
		}
	}

	/// Store a batch of blocks. This should be more efficient than storing them one by one.
	/// Assumes that the blocks are already sorted by block number.
	/// This method delegates the storage operation to the appropriate
	/// database backend based on the internal state implementation.
	///
	/// # Arguments
	///
	/// * `blocks` - A vector containing the blocks to be stored.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn batch_store_blocks(&self, blocks: Vec<Block>) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(_s) => todo!(),
			StateInternalImpl::StatePg(s) => s.batch_store_blocks(blocks).await,
			StateInternalImpl::StateCas(s) => s.batch_store_blocks(blocks).await,
		}
	}

	/// Stores a transaction in the database.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number associated with the transaction.
	/// * `block_hash` - The hash of the block containing the transaction.
	/// * `transaction` - The transaction to store.
	/// * `tx_sequence` - The sequence number of the transaction within the block.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn store_transaction(
		&self,
		block_number: BlockNumber,
		block_hash: BlockHash,
		transaction: Transaction,
		tx_sequence: TransactionSequence,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_transaction(block_number, block_hash, transaction, tx_sequence).await,
			StateInternalImpl::StatePg(s) =>
				s.store_transaction(block_number, block_hash, transaction, tx_sequence).await,
			StateInternalImpl::StateCas(s) =>
				s.store_transaction(block_number, block_hash, transaction, tx_sequence).await,
		}
	}

	/// Stores a transaction metadata in the database.
	///
	/// # Arguments
	///
	/// * `tx_hash` - Hash of the transaction.
	/// * `fee_used` - Fee used in transaction execution.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn store_transaction_metadata(
		&self,
		metadata: TransactionMetadata
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_transaction_metadata(metadata).await,
			StateInternalImpl::StatePg(s) =>
				s.store_transaction_metadata(metadata).await,
			StateInternalImpl::StateCas(s) =>
				s.store_transaction_metadata(metadata).await,
		}
	}

	/// Stores multiple transactions in batch mode.
	///
	/// # Arguments
	///
	/// * `transactions` - A vector of transaction tuples containing (block_number, block_hash,
	///   transaction).
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn batch_store_transactions(
		&self,
		transactions: Vec<(BlockNumber, BlockHash, Transaction)>,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.batch_store_transactions(transactions).await,
			StateInternalImpl::StatePg(s) => s.batch_store_transactions(transactions).await,
			StateInternalImpl::StateCas(s) => s.batch_store_transactions(transactions).await,
		}
	}

	/// Retrieves the transaction count for a given verifying key and block number.
	///
	/// # Arguments
	///
	/// * `verifying_key` - The address of the verifying key.
	/// * `block_number` - An optional block number to filter transactions.
	///
	/// # Returns
	///
	/// A `Result` containing the transaction count if successful, otherwise an `Error`.
	pub async fn get_transaction_count(
		&self,
		verifying_key: &Address,
		block_number: Option<BlockNumber>,
	) -> Result<u64, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.get_transaction_count(verifying_key, block_number).await,
			StateInternalImpl::StatePg(s) =>
				s.get_transaction_count(verifying_key, block_number).await,
			StateInternalImpl::StateCas(s) =>
				s.get_transaction_count(verifying_key, block_number).await,
		}
	}

	/// Loads the block associated with the given block number and cluster address.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number to load.
	/// * `cluster_address` - The address of the cluster.
	///
	/// # Returns
	///
	/// A `Result` containing the loaded block if successful, otherwise an `Error`.
	pub async fn load_block(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<Block, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_block(block_number, cluster_address).await,
			StateInternalImpl::StatePg(s) => s.load_block(block_number, cluster_address).await,
			StateInternalImpl::StateCas(s) => s.load_block(block_number, cluster_address).await,
		}
	}

	/// Loads the block number with the given block hash.
	///
	/// # Arguments
	///
	/// * `block_hash` - Hash of the block.
	///
	/// # Returns
	///
	/// A `Result` containing the block number if successful, otherwise an `Error`.
	pub async fn get_block_number_by_hash(
		&self,
		block_hash: BlockHash,
	) -> Result<BlockNumber, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_block_number_by_hash(block_hash).await,
			StateInternalImpl::StatePg(s) => s.get_block_number_by_hash(block_hash).await,
			StateInternalImpl::StateCas(s) => s.get_block_number_by_hash(block_hash).await,
		}
	}

	/// Loads the block response associated with the given block number and cluster address.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number to load.
	/// * `cluster_address` - The address of the cluster.
	///
	/// # Returns
	///
	/// A `Result` containing the loaded block response if successful, otherwise an `Error`.
	pub async fn load_block_response(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<BlockResponse, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.load_block_response(block_number, cluster_address).await,
			StateInternalImpl::StatePg(s) =>
				s.load_block_response(block_number, cluster_address).await,
			StateInternalImpl::StateCas(s) =>
				s.load_block_response(block_number, cluster_address).await,
		}
	}

	/// Stores a block header in the database.
	///
	/// # Arguments
	///
	/// * `block_header` - The block header to store.
	///
	/// # Returns
	///
	/// A `Result` indicating success or failure.
	pub async fn store_block_header(&self, block_header: BlockHeader) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.store_block_header(block_header).await,
			StateInternalImpl::StatePg(s) => s.store_block_header(block_header).await,
			StateInternalImpl::StateCas(s) => s.store_block_header(block_header).await,
		}
	}

	/// Loads the block header for a given block number and cluster address.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number of the header to load.
	/// * `cluster_address` - The address of the cluster.
	///
	/// # Returns
	///
	/// Returns a `BlockHeader` if the header is found, otherwise returns an `Error`.
	pub async fn load_block_header(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<BlockHeader, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.load_block_header(block_number, cluster_address).await,
			StateInternalImpl::StatePg(s) =>
				s.load_block_header(block_number, cluster_address).await,
			StateInternalImpl::StateCas(s) =>
				s.load_block_header(block_number, cluster_address).await,
		}
	}

	/// Loads the transaction associated with a given transaction hash.
	///
	/// # Arguments
	///
	/// * `transaction_hash` - The hash of the transaction to load.
	///
	/// # Returns
	///
	/// Returns `Some(Transaction)` if the transaction is found, otherwise returns `None`.
	pub async fn load_transaction(
		&self,
		transaction_hash: TransactionHash,
	) -> Result<Option<Transaction>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_transaction(transaction_hash).await,
			StateInternalImpl::StatePg(s) => s.load_transaction(transaction_hash).await,
			StateInternalImpl::StateCas(s) => s.load_transaction(transaction_hash).await,
		}
	}

	/// Loads the transaction receipt associated with a given transaction hash.
	///
	/// # Arguments
	///
	/// * `transaction_hash` - The hash of the transaction to load the receipt for.
	///
	/// # Returns
	///
	/// Returns `Some(TransactionReceiptResponse)` if the receipt is found, otherwise returns
	/// `None`.
	pub async fn load_transaction_receipt(
		&self,
		transaction_hash: TransactionHash,
		cluster_address: Address,
	) -> Result<Option<TransactionReceiptResponse>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_transaction_receipt(transaction_hash, cluster_address).await,
			StateInternalImpl::StatePg(s) => s.load_transaction_receipt(transaction_hash, cluster_address).await,
			StateInternalImpl::StateCas(s) => s.load_transaction_receipt(transaction_hash, cluster_address).await,
		}
	}

	/// Loads transaction receipts for transactions associated with a given address.
	///
	/// # Arguments
	///
	/// * `address` - The address for which transaction receipts are to be loaded.
	///
	/// # Returns
	///
	/// Returns a vector of `TransactionReceiptResponse` objects representing the transaction
	/// receipts.
	pub async fn load_transaction_receipt_by_address(
		&self,
		address: Address,
	) -> Result<Vec<TransactionReceiptResponse>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_transaction_receipt_by_address(address).await,
			StateInternalImpl::StatePg(s) => s.load_transaction_receipt_by_address(address).await,
			StateInternalImpl::StateCas(s) => s.load_transaction_receipt_by_address(address).await,
		}
	}

	/// Loads the latest block headers for a given cluster address.
	///
	/// # Arguments
	///
	/// * `num_blocks` - The number of latest block headers to load.
	/// * `cluster_address` - The address of the cluster for which block headers are to be loaded.
	///
	/// # Returns
	///
	/// Returns a vector of `BlockHeader` objects representing the latest block headers.
	pub async fn load_latest_block_headers(
		&self,
		num_blocks: u32,
		cluster_address: Address,
	) -> Result<Vec<rpc_model::BlockHeaderV3>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.load_latest_block_headers(num_blocks, cluster_address).await,
			StateInternalImpl::StatePg(s) =>
				s.load_latest_block_headers(num_blocks, cluster_address).await,
			StateInternalImpl::StateCas(s) =>
				s.load_latest_block_headers(num_blocks, cluster_address).await,
		}
	}

	/// Loads the latest transactions up to a specified number.
	///
	/// # Arguments
	///
	/// * `num_transactions` - The maximum number of latest transactions to load.
	///
	/// # Returns
	///
	/// Returns a vector of `TransactionResponse` objects representing the latest transactions.
	pub async fn load_latest_transactions(
		&self,
		num_transactions: u32,
	) -> Result<Vec<TransactionResponse>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_latest_transactions(num_transactions).await,
			StateInternalImpl::StatePg(s) => s.load_latest_transactions(num_transactions).await,
			StateInternalImpl::StateCas(s) => s.load_latest_transactions(num_transactions).await,
		}
	}

	/// Loads transactions for a given block number.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number for which transactions are to be loaded.
	///
	/// # Returns
	///
	/// Returns a vector of `Transaction` objects representing the transactions for the specified
	/// block number.
	pub async fn load_transactions(
		&self,
		block_number: BlockNumber,
	) -> Result<Vec<Transaction>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_transactions(block_number).await,
			StateInternalImpl::StatePg(s) => s.load_transactions(block_number).await,
			StateInternalImpl::StateCas(s) => s.load_transactions(block_number).await,
		}
	}

	/// Loads transaction receipt responses for transactions associated with a given block number.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number for which transaction receipt responses are to be
	///   loaded.
	///
	/// # Returns
	///
	/// Returns a vector of `TransactionReceiptResponse` objects representing the transaction
	/// receipt responses.
	pub async fn load_transactions_response(
		&self,
		block_number: BlockNumber,
	) -> Result<Vec<TransactionReceiptResponse>, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.load_transactions_response(block_number).await,
			StateInternalImpl::StatePg(s) => s.load_transactions_response(block_number).await,
			StateInternalImpl::StateCas(s) => s.load_transactions_response(block_number).await,
		}
	}

	/// Checks whether the given cluster address corresponds to a block head.
	///
	/// # Arguments
	///
	/// * `cluster_address` - The address of the cluster to check.
	///
	/// # Returns
	///
	/// Returns `true` if the cluster address corresponds to a block head, `false` otherwise.
	pub async fn is_block_head(&self, cluster_address: &Address) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.is_block_head(cluster_address).await,
			StateInternalImpl::StatePg(s) => s.is_block_head(cluster_address).await,
			StateInternalImpl::StateCas(s) => s.is_block_head(cluster_address).await,
		}
	}

	/// Checks whether the specified block number has been executed in the given cluster.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number to check for execution.
	/// * `cluster_address` - The address of the cluster in which to check for execution.
	///
	/// # Returns
	///
	/// Returns `true` if the block has been executed in the specified cluster, `false` otherwise.
	pub async fn is_block_executed(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.is_block_executed(block_number, cluster_address).await,
			StateInternalImpl::StatePg(s) =>
				s.is_block_executed(block_number, cluster_address).await,
			StateInternalImpl::StateCas(s) =>
				s.is_block_executed(block_number, cluster_address).await,
		}
	}

	/// Checks whether the specified block is stored or not.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number to check for execution.
	///
	/// # Returns
	///
	/// Returns `true` if the block is stored, `false` otherwise.
	pub async fn is_block_header_stored(
		&self,
		block_number: BlockNumber,
	) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.is_block_header_stored(block_number).await,
			StateInternalImpl::StatePg(s) =>
				s.is_block_header_stored(block_number).await,
			StateInternalImpl::StateCas(s) =>
				s.is_block_header_stored(block_number).await,
		}
	}

	/// Sets the execution status of the specified block number in the given cluster.
	///
	/// # Arguments
	///
	/// * `block_number` - The block number for which to set the execution status.
	/// * `cluster_address` - The address of the cluster in which to set the execution status.
	///
	/// # Returns
	///
	/// Returns `Ok(())` if the operation is successful, or an `Error` otherwise.
	pub async fn set_block_executed(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.set_block_executed(block_number, cluster_address).await,
			StateInternalImpl::StatePg(s) =>
				s.set_block_executed(block_number, cluster_address).await,
			StateInternalImpl::StateCas(s) =>
				s.set_block_executed(block_number, cluster_address).await,
		}
	}

	/// Queries the maximum epoch from the block_header table.
	///
	///
	/// # Returns
	///
	/// Returns `Ok(Epoch, BlockNumber)` if the operation is successful, or an `Error` otherwise.
	pub async fn load_genesis_block_time(
		&self
	) -> Result<(u64, u64), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.load_genesis_block_time().await,
			StateInternalImpl::StatePg(s) =>
				s.load_genesis_block_time().await,
			StateInternalImpl::StateCas(s) =>
				s.load_genesis_block_time().await,
		}
	}

	pub async fn store_genesis_block_time(
		&self,
		block_number: BlockNumber,
		epoch: Epoch,
	) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) =>
				s.store_genesis_block_time(block_number, epoch).await,
			StateInternalImpl::StatePg(s) =>
				s.store_genesis_block_time(block_number, epoch).await,
			StateInternalImpl::StateCas(s) =>
				s.store_genesis_block_time(block_number, epoch).await,
		}
	}
}
