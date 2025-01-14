use ethereum_types::{H160, H256, U256, U64};
use jsonrpsee::{core::SubscriptionResult, proc_macros::rpc, types::ErrorObjectOwned};
use types::eth::{
	block::{Block, BlockNumber},
	bytes::Bytes,
	filter::{Kind, Params},
	log::Log,
	signers::{EvmEthCallRequest, TransactionRequest},
	sync::SyncStatus,
	transaction::{Transaction, TransactionReceipt},
};

#[rpc(server, namespace = "eth")]
pub trait EvmCompatibility {
	#[method(name = "syncing")]
	async fn syncing(&self) -> Result<SyncStatus, ErrorObjectOwned>;

	#[method(name = "coinbase")]
	async fn coinbase(&self) -> Result<H160, ErrorObjectOwned>;

	#[method(name = "getStorageAt")]
	async fn get_storage_at(
		&self,
		address: H160,
		index: H256,
		block: BlockNumber,
	) -> Result<H256, ErrorObjectOwned>;

	#[method(name = "getBalance")]
	async fn balance(
		&self,
		address: H160,
		block_number: Option<BlockNumber>,
	) -> Result<U256, ErrorObjectOwned>;

	#[method(name = "maxPriorityFeePerGas")]
	async fn max_priority_fee_per_gas(&self) -> Result<U256, ErrorObjectOwned>;

	#[method(name = "getLogs")]
	async fn get_logs(&self, filter: serde_json::Value) -> Result<Vec<Log>, ErrorObjectOwned>;

	#[method(name = "getCode")]
	async fn get_code(
		&self,
		address: H160,
		block: Option<BlockNumber>,
	) -> Result<Bytes, ErrorObjectOwned>;

	#[method(name = "call")]
	async fn call(
		&self,
		request: EvmEthCallRequest,
		block: BlockNumber,
	) -> Result<Bytes, ErrorObjectOwned>;

	/// Note:
	/// need name = "subscription" as the method of the returned events needs to be
	/// "eth_subscription" however, need to use "eth_subscribe" as the method name in the request
	/// but for name = "subscription", we need unsubscribe specified as well, not sure yet if it
	/// should be "unsubscribe" or "eth_unsubscribe"
	#[subscription(name = "subscription", aliases = ["eth_subscribe"], item = String, unsubscribe = "unsubscribe")]
	async fn subscribe(&self, kind: Kind, params: Option<Params>) -> SubscriptionResult;

	#[method(name = "blockNumber")]
	async fn block_number(&self) -> Result<String, ErrorObjectOwned>;

	#[method(name = "chainId")]
	async fn chain_id(&self) -> Result<Option<U64>, ErrorObjectOwned>;

	// This method should defined in "net" namespace. But because `FullNodeJsonImpl`
	// can't by cloned, the workaround with `aliases` is used. `aliases` ignores the current namespace
	#[method(name = "net_version", aliases = ["net_version"])]
	async fn net_version(&self) -> Result<Option<String>, ErrorObjectOwned>;

	#[method(name = "feeHistory")]
	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumber,
		reward_percentiles: Vec<f64>,
	) -> Result<ethers::types::FeeHistory, ErrorObjectOwned>;

	#[method(name = "estimateGas")]
	async fn estimate_gas(
		&self,
		eth_call: TransactionRequest,
		block_number: Option<BlockNumber>,
	) -> Result<U256, ErrorObjectOwned>;

	#[method(name = "gasPrice")]
	async fn gas_price(&self) -> Result<U256, ErrorObjectOwned>;

	#[method(name = "getBlockByNumber")]
	async fn get_block_by_number_eth(
		&self,
		block_number: BlockNumber,
		include_tx: bool,
	) -> Result<Option<Block>, ErrorObjectOwned>;

	#[method(name = "getBlockByHash")]
	async fn get_block_by_hash(
		&self,
		block_hash: H256,
		include_tx: bool,
	) -> Result<Option<Block>, ErrorObjectOwned>;

	#[method(name = "getTransactionCount")]
	async fn get_transaction_count(
		&self,
		address: H160,
		number: Option<BlockNumber>,
	) -> Result<U256, ErrorObjectOwned>;

	#[method(name = "getBlockTransactionCountByNumber")]
	async fn get_block_transaction_count_by_number(
		&self,
		block: BlockNumber,
	) -> Result<U256, ErrorObjectOwned>;

	#[method(name = "getTransactionByHash")]
	async fn transaction_by_hash(
		&self,
		hash: H256,
	) -> Result<Option<Transaction>, ErrorObjectOwned>;

	#[method(name = "getTransactionReceipt")]
	async fn transaction_receipt(
		&self,
		hash: H256,
	) -> Result<Option<TransactionReceipt>, ErrorObjectOwned>;

	#[method(name = "sendRawTransaction")]
	async fn send_raw_transaction(&self, bytes: Bytes) -> Result<H256, ErrorObjectOwned>;
}