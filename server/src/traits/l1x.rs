use jsonrpsee::{core::SubscriptionResult, proc_macros::rpc, types::ErrorObjectOwned};
use l1x_rpc::rpc_model::{
	EstimateFeeRequest, EstimateFeeResponse, GetAccountStateRequest, GetAccountStateResponse, GetBlockByNumberRequest,
	GetBlockV3ByNumberResponse, GetChainStateRequest, GetChainStateResponse, GetCurrentNonceRequest,
	GetCurrentNonceResponse, GetEventsRequest, GetEventsResponse, GetLatestBlockHeadersRequest,
	GetLatestTransactionsRequest, GetStakeRequest, GetStakeResponse, GetTransactionReceiptRequest,
	GetTransactionReceiptResponse, GetTransactionV3ReceiptResponse, GetTransactionsByAccountRequest,
	GetTransactionsByAccountResponse, GetTransactionsV3ByAccountResponse, SmartContractReadOnlyCallRequest,
	SmartContractReadOnlyCallResponse, SubmitTransactionRequest, SubmitTransactionRequestV2, SubmitTransactionResponse,
	GetCurrentNodeInfoRequest, GetCurrentNodeInfoResponse, GetBlockInfoRequest, GetBlockInfoResponse,
	GetRuntimeConfigRequest, GetRuntimeConfigResponse, GetBlockWithDetailsByNumberRequest, GetBlockWithDetailsByNumberResponse,
};
use types::eth::filter::{Kind, Params};

#[rpc(server, namespace = "l1x")]
pub trait FullNodeJson {
	#[method(name = "getAccountState")]
	async fn get_account_state(
		&self,
		request: GetAccountStateRequest,
	) -> Result<GetAccountStateResponse, ErrorObjectOwned>;

	#[method(name = "submitTransaction")]
	async fn submit_transaction(
		&self,
		request: SubmitTransactionRequest,
	) -> Result<SubmitTransactionResponse, ErrorObjectOwned>;

	#[method(name = "submitTransactionV2")]
	async fn submit_transaction_v2(
		&self,
		request: SubmitTransactionRequestV2,
	) -> Result<SubmitTransactionResponse, ErrorObjectOwned>;

	#[method(name = "estimateFee")]
	async fn estimate_fee(
		&self,
		request: EstimateFeeRequest,
	) -> Result<EstimateFeeResponse, ErrorObjectOwned>;

	#[method(name = "getTransactionReceipt")]
	async fn get_transaction_receipt(
		&self,
		request: GetTransactionReceiptRequest,
	) -> Result<GetTransactionReceiptResponse, ErrorObjectOwned>;

	#[method(name = "getTransactionV3Receipt")]
	async fn get_transaction_v3_receipt(
		&self,
		request: GetTransactionReceiptRequest,
	) -> Result<GetTransactionV3ReceiptResponse, ErrorObjectOwned>;

	#[method(name = "getTransactionsByAccount")]
	async fn get_transactions_by_account(
		&self,
		request: GetTransactionsByAccountRequest,
	) -> Result<GetTransactionsByAccountResponse, ErrorObjectOwned>;

	#[method(name = "getTransactionsV3ByAccount")]
	async fn get_transactions_v3_by_account(
		&self,
		request: GetTransactionsByAccountRequest,
	) -> Result<GetTransactionsV3ByAccountResponse, ErrorObjectOwned>;

	#[method(name = "smartContractReadOnlyCall")]
	async fn smart_contract_read_only_call(
		&self,
		request: SmartContractReadOnlyCallRequest,
	) -> Result<SmartContractReadOnlyCallResponse, ErrorObjectOwned>;

	#[method(name = "getChainState")]
	async fn get_chain_state(
		&self,
		request: GetChainStateRequest,
	) -> Result<GetChainStateResponse, ErrorObjectOwned>;

	#[method(name = "getBlockByNumber")]
	async fn get_block_by_number(
		&self,
		request: GetBlockByNumberRequest,
	) -> Result<GetBlockV3ByNumberResponse, ErrorObjectOwned>;

	#[subscription(name = "getLatestBlockHeaders", item = String, unsubscribe = "unsubscribeLatestBlockHeaders")]
	async fn get_latest_block_headers(
		&self,
		request:GetLatestBlockHeadersRequest,
	) -> SubscriptionResult;

	#[subscription(name = "getLatestTransactions", item = String, unsubscribe = "unsubscribeLatestTransactions")]
	async fn get_latest_transactions(
		&self,
		request: GetLatestTransactionsRequest,
	) -> SubscriptionResult;

	#[method(name = "getStake")]
	async fn get_stake(
		&self,
		request: GetStakeRequest,
	) -> Result<GetStakeResponse, ErrorObjectOwned>;

	#[method(name = "getCurrentNonce")]
	async fn get_current_nonce(
		&self,
		request: GetCurrentNonceRequest,
	) -> Result<GetCurrentNonceResponse, ErrorObjectOwned>;

	#[method(name = "getEvents")]
	async fn get_events(
		&self,
		request: GetEventsRequest,
	) -> Result<GetEventsResponse, ErrorObjectOwned>;

	#[subscription(name = "subscribeEvents", item = String)]
	async fn subscribe_events(&self, kind: Kind, params: Option<Params>) -> SubscriptionResult;

	#[method(name = "getCurrentNodeInfo")]
	async fn get_current_node_info(
		&self,
		request: GetCurrentNodeInfoRequest,
	) -> Result<GetCurrentNodeInfoResponse, ErrorObjectOwned>;

	#[method(name = "getBlockInfo")]
	async fn get_block_info(
		&self,
		request: GetBlockInfoRequest,
	) -> Result<GetBlockInfoResponse, ErrorObjectOwned>;

	#[method(name = "getRuntimeConfig")]
	async fn get_runtime_config(
		&self,
		request: GetRuntimeConfigRequest,
	) -> Result<GetRuntimeConfigResponse, ErrorObjectOwned>;

	#[method(name = "getBlockWithDetailsByNumber")]
	async fn get_block_with_details_by_number(
		&self,
		request: GetBlockWithDetailsByNumberRequest,
	) -> Result<GetBlockWithDetailsByNumberResponse, ErrorObjectOwned>;
}
