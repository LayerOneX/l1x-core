use anyhow::Error;
use async_trait::async_trait;
use l1x_rpc::rpc_model::{self, TransactionResponse};
use primitives::*;

use system::{
	block::{Block, BlockResponse},
	block_header::BlockHeader,
	chain_state::ChainState,
	transaction::{Transaction, TransactionMetadata},
	transaction_receipt::TransactionReceiptResponse,
};

#[async_trait]
pub trait BlockState {
	async fn store_block_head(
		&self,
		cluster_address: &Address,
		block_number: BlockNumber,
		block_hash: BlockHash,
	) -> Result<(), Error>;

	async fn update_block_head(
		&self,
		cluster_address: Address,
		big_block_number: BlockNumber,
		block_hash: BlockHash,
	) -> Result<(), Error>;

	async fn load_chain_state(&self, cluster_address: Address) -> Result<ChainState, Error>;

	async fn block_head(&self, cluster_address: Address) -> Result<Block, Error>;

	async fn block_head_header(&self, cluster_address: Address) -> Result<BlockHeader, Error>;

	async fn store_block(&self, block: Block) -> Result<(), Error>;

	async fn batch_store_blocks(&self, blocks: Vec<Block>) -> Result<(), Error>;

	async fn store_transaction(
		&self,
		block_number: BlockNumber,
		block_hash: BlockHash,
		transaction: Transaction,
		_tx_order: TransactionSequence,
	) -> Result<(), Error>;

	async fn batch_store_transactions(
		&self,
		transactions: Vec<(BlockNumber, BlockHash, Transaction)>,
	) -> Result<(), Error>;

	async fn store_transaction_metadata(
		&self,
		metadata: TransactionMetadata
	) -> Result<(), Error>;

	async fn load_transaction_metadata(
		&self,
		tx_hash: TransactionHash,
	) -> Result<TransactionMetadata, Error>;

	async fn get_transaction_count(
		&self,
		verifying_key: &Address,
		block_number: Option<BlockNumber>,
	) -> Result<u64, Error>;

	async fn load_block(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<Block, Error>;

	async fn get_block_number_by_hash(
		&self,
		block_hash: BlockHash,
	) -> Result<BlockNumber, Error>;

	async fn load_block_response(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<BlockResponse, Error>;

	async fn store_block_header(&self, block_header: BlockHeader) -> Result<(), Error>;

	async fn batch_store_block_headers(&self, block_headers: Vec<BlockHeader>)
		-> Result<(), Error>;

	async fn load_block_header(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<BlockHeader, Error>;

	async fn load_transaction(
		&self,
		transaction_hash: TransactionHash,
	) -> Result<Option<Transaction>, Error>;

	async fn load_transaction_receipt(
		&self,
		transaction_hash: TransactionHash,
		cluster_address: Address
	) -> Result<Option<TransactionReceiptResponse>, Error>;

	async fn load_transaction_receipt_by_address(
		&self,
		address: Address,
	) -> Result<Vec<TransactionReceiptResponse>, Error>;

	async fn load_latest_block_headers(
		&self,
		num_blocks: u32,
		cluster_address: Address,
	) -> Result<Vec<rpc_model::BlockHeaderV3>, Error>;

	async fn load_latest_transactions(
		&self,
		num_transactions: u32,
	) -> Result<Vec<TransactionResponse>, Error>;

	async fn load_transactions(&self, block_number: BlockNumber)
		-> Result<Vec<Transaction>, Error>;

	async fn load_transactions_response(
		&self,
		block_number: BlockNumber,
	) -> Result<Vec<TransactionReceiptResponse>, Error>;

	async fn is_block_head(&self, cluster_address: &Address) -> Result<bool, Error>;

	async fn is_block_executed(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<bool, Error>;

	async fn is_block_header_stored(
		&self,
		blk_number: BlockNumber,
	) -> Result<bool, Error>;

	async fn set_block_executed(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<(), Error>;

	async fn load_genesis_block_time(
		&self,
	) -> Result<(u64, u64), Error>;
	
	async fn store_genesis_block_time(
		&self,
		blk_number: BlockNumber,
		epoch: Epoch,
	) -> Result<(), Error>;
}
