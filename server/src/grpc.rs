use std::sync::Arc;
use crate::service::{FullNodeService, parse_block_number};
use l1x_rpc::rpc_model::{node_server::{Node, NodeServer}, CreateAccountRequest, CreateAccountResponse,
						 EstimateFeeRequest, EstimateFeeResponse, GetAccountStateRequest, GetAccountStateResponse,
						 GetBlockByNumberRequest, GetBlockByNumberResponse, GetBlockV2ByNumberResponse,
						 GetBlockV3ByNumberResponse, GetChainStateRequest, GetChainStateResponse, GetCurrentNonceRequest,
						 GetCurrentNonceResponse, GetEventsRequest, GetEventsResponse, GetLatestBlocksRequest,
						 GetLatestBlocksResponse, GetProtocolVersionRequest, GetProtocolVersionResponse, GetStakeRequest,
						 GetStakeResponse, GetTransactionReceiptRequest, GetTransactionReceiptResponse,
						 GetTransactionV3ReceiptResponse, GetTransactionsByAccountRequest, GetTransactionsByAccountResponse,
						 GetTransactionsV3ByAccountResponse, ImportAccountRequest, ImportAccountResponse,
						 SmartContractReadOnlyCallRequest, SmartContractReadOnlyCallResponse, SubmitTransactionRequest,
						 SubmitTransactionRequestV2, SubmitTransactionResponse, GetNodeInfoRequest, GetNodeInfoResponse,
						 GetGenesisBlockRequest, GetGenesisBlockResponse, GetCurrentNodeInfoRequest, GetCurrentNodeInfoResponse,
						 GetNodeHealthsRequest, GetNodeHealthsResponse, GetBpForEpochRequest, GetBpForEpochResponse,
						 GetValidatorsForEpochRequest, GetValidatorsForEpochResponse, GetBlockInfoRequest,
						 GetBlockInfoResponse, GetRuntimeConfigRequest, GetRuntimeConfigResponse, GetBlockWithDetailsByNumberRequest,
						 GetBlockWithDetailsByNumberResponse,
};
use log::info;
use moka::sync::Cache;
use tokio::sync::mpsc;

use system::{config::GrpcConfig, mempool::ResponseMempool};
use tonic::{codegen::http::Method, transport::Server, Request, Response, Status};
use tonic_web::GrpcWebLayer;
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};
use primitives::BlockNumber;

#[allow(dead_code)]
pub struct FullNodeGrpc {
	service: FullNodeService,
	mempool_res_rx: mpsc::Receiver<ResponseMempool>,
	block_cache: Arc<Cache<BlockNumber, GetBlockV3ByNumberResponse>>,
}

/// gRPC wrapper for the FullNodeService
#[tonic::async_trait]
impl Node for FullNodeGrpc {
	type GetEventsStream =
		tokio_stream::wrappers::ReceiverStream<Result<GetEventsResponse, Status>>;
	type SubmitTransactionStream =
		tokio_stream::wrappers::ReceiverStream<Result<SubmitTransactionResponse, Status>>;
	type SubmitTransactionV2Stream =
		tokio_stream::wrappers::ReceiverStream<Result<SubmitTransactionResponse, Status>>;
	async fn create_account(
		&self,
		request: Request<CreateAccountRequest>,
	) -> Result<Response<CreateAccountResponse>, Status> {
		Ok(Response::new(self.service.new_account(request.into_inner()).await?))
	}
	async fn import_account(
		&self,
		request: Request<ImportAccountRequest>,
	) -> Result<Response<ImportAccountResponse>, Status> {
		Ok(Response::new(self.service.import_account(request.into_inner()).await?))
	}
	async fn get_account_state(
		&self,
		request: Request<GetAccountStateRequest>,
	) -> Result<Response<GetAccountStateResponse>, Status> {
		Ok(Response::new(self.service.get_account_state(request.into_inner()).await?))
	}

	async fn submit_transaction(
		&self,
		request: Request<SubmitTransactionRequest>,
	) -> Result<Response<Self::
	SubmitTransactionStream>, Status> {
		Ok(Response::new(self.service.submit_transaction(request.into_inner().into()).await?))
	}

	async fn submit_transaction_v2(
		&self,
		request: Request<SubmitTransactionRequestV2>,
	) -> Result<Response<Self::
	SubmitTransactionStream>, Status> {
		Ok(Response::new(self.service.submit_transaction(request.into_inner()).await?))
	}

	async fn estimate_fee(
		&self,
		request: Request<EstimateFeeRequest>,
	) -> Result<Response<EstimateFeeResponse>, Status> {
		Ok(Response::new(self.service.estimate_fee(request.into_inner()).await?))
	}

	async fn get_transaction_receipt(
		&self,
		request: Request<GetTransactionReceiptRequest>,
	) -> Result<Response<GetTransactionReceiptResponse>, Status> {
		Ok(Response::new(self.service.get_transaction_receipt(request.into_inner()).await?))
	}

	async fn get_transaction_v3_receipt(
		&self,
		request: Request<GetTransactionReceiptRequest>,
	) -> Result<Response<GetTransactionV3ReceiptResponse>, Status> {
		Ok(Response::new(self.service.get_transaction_v3_receipt(request.into_inner()).await?))
	}

	async fn get_transactions_by_account(
		&self,
		request: Request<GetTransactionsByAccountRequest>,
	) -> Result<Response<GetTransactionsByAccountResponse>, Status> {
		Ok(Response::new(self.service.get_transactions_by_account(request.into_inner()).await?))
	}

	async fn get_transactions_v3_by_account(
		&self,
		request: Request<GetTransactionsByAccountRequest>,
	) -> Result<Response<GetTransactionsV3ByAccountResponse>, Status> {
		Ok(Response::new(self.service.get_transactions_v3_by_account(request.into_inner()).await?))
	}

	async fn smart_contract_read_only_call(
		&self,
		request: Request<SmartContractReadOnlyCallRequest>,
	) -> Result<Response<SmartContractReadOnlyCallResponse>, Status> {
		Ok(Response::new(self.service.smart_contract_read_only_call(request.into_inner()).await?))
	}

	async fn get_chain_state(
		&self,
		request: Request<GetChainStateRequest>,
	) -> Result<Response<GetChainStateResponse>, Status> {
		Ok(Response::new(self.service.get_chain_state(request.into_inner()).await?))
	}

	async fn get_block_by_number(
		&self,
		request: Request<GetBlockByNumberRequest>,
	) -> Result<Response<GetBlockByNumberResponse>, Status> {
		Ok(Response::new(self.service.get_block_by_number(request.into_inner()).await?))
	}

	// This gRPC is responsible for fetching block with transaction version 2
	async fn get_block_v2_by_number(
		&self,
		request: Request<GetBlockByNumberRequest>,
	) -> Result<Response<GetBlockV2ByNumberResponse>, Status> {
		Ok(Response::new(self.service.get_block_v2_by_number(request.into_inner()).await?))
	}

	// This gRPC is responsible for fetching block with transaction version 2
	async fn get_block_v3_by_number(
		&self,
		request: Request<GetBlockByNumberRequest>,
	) -> Result<Response<GetBlockV3ByNumberResponse>, Status> {
		let request = request.into_inner();
		let block_number = parse_block_number(&request.block_number).await?;
		// Check if the response for the requested block number is already in the cache
		if let Some(cached_response) = self.block_cache.get(&block_number) {
			return Ok(Response::new(cached_response));
		}
		let block_v3_by_number_response = self.service.get_block_v3_by_number(request).await?;
		// Cache the response for future requests
		self.block_cache.insert(block_number, block_v3_by_number_response.clone());
		Ok(Response::new(block_v3_by_number_response))
	}

	/*
	async fn get_latest_block_headers(
		&self,
		request: Request<GetLatestBlockHeadersRequest>,
	) -> Result<Response<GetLatestBlockHeadersResponse>, Status> { Ok(Response::new(
	  self.service.get_latest_block_headers(request.into_inner()).await?, ))
	}

	async fn get_latest_transactions(
		&self,
		request: Request<GetLatestTransactionsRequest>,
	) -> Result<Response<GetLatestTransactionsResponse>, Status> { //
	  self.service.get_latest_transactions(request.into_inner()).await?;

	}
	*/
	async fn get_stake(
		&self,
		request: Request<GetStakeRequest>,
	) -> Result<Response<GetStakeResponse>, Status> {
		Ok(Response::new(self.service.get_stake(request.into_inner()).await?))
	}

	async fn get_current_nonce(
		&self,
		request: Request<GetCurrentNonceRequest>,
	) -> Result<Response<GetCurrentNonceResponse>, Status> {
		Ok(Response::new(self.service.get_current_nonce(request.into_inner()).await?))
	}

	async fn get_events(
		&self,
		request: Request<GetEventsRequest>,
	) -> Result<Response<Self::GetEventsStream>, Status> {
		Ok(Response::new(self.service.get_events(request.into_inner()).await?))
	}

	async fn get_latest_blocks(&self, request: Request<GetLatestBlocksRequest>) -> Result<Response<GetLatestBlocksResponse>, Status> {
		Ok(Response::new(self.service.get_latest_blocks(request.into_inner()).await?))
	}

	async fn get_protocol_version(&self, request: Request<GetProtocolVersionRequest>) -> Result<Response<GetProtocolVersionResponse>, Status> {
		Ok(Response::new(self.service.get_protocol_version(request.into_inner()).await?))
	}

	async fn get_node_info(&self,request: Request<GetNodeInfoRequest>) -> Result<Response<GetNodeInfoResponse>, Status> {
        Ok(Response::new(self.service.get_node_info(request.into_inner()).await?))
    }
	async fn get_genesis_block(&self, request: Request<GetGenesisBlockRequest>) -> Result<Response<GetGenesisBlockResponse>, Status> {
		Ok(Response::new(self.service.get_genesis_block(request.into_inner()).await?))
    }

	async fn get_current_node_info(&self, request: Request<GetCurrentNodeInfoRequest>) -> Result<Response<GetCurrentNodeInfoResponse>, Status> {
		Ok(Response::new(self.service.get_current_node_info(request.into_inner()).await?))
	}

	async fn get_node_healths(&self, request: Request<GetNodeHealthsRequest>) -> Result<Response<GetNodeHealthsResponse>, Status> {
		Ok(Response::new(self.service.get_node_healths(request.into_inner()).await?))
	}

	async fn get_block_proposer_for_epoch(&self, request: Request<GetBpForEpochRequest>) -> Result<Response<GetBpForEpochResponse>, Status> {
		Ok(Response::new(self.service.get_block_proposer_for_epoch(request.into_inner()).await?))
	}

	async fn get_validators_for_epoch(&self, request: Request<GetValidatorsForEpochRequest>) -> Result<Response<GetValidatorsForEpochResponse>, Status> {
		Ok(Response::new(self.service.get_validators_for_epoch(request.into_inner()).await?))
	}

	async fn get_block_info(&self, request: Request<GetBlockInfoRequest>) -> Result<Response<GetBlockInfoResponse>, Status> {
		Ok(Response::new(self.service.get_block_info(request.into_inner()).await?))
	}

	async fn get_runtime_config(&self, request: Request<GetRuntimeConfigRequest>) -> Result<Response<GetRuntimeConfigResponse>, Status> {
		Ok(Response::new(self.service.get_runtime_config(request.into_inner()).await?))
	}

	async fn get_block_with_details_by_number(&self, request: Request<GetBlockWithDetailsByNumberRequest>) -> Result<Response<GetBlockWithDetailsByNumberResponse>, Status> {
		Ok(Response::new(self.service.get_block_with_details_by_number(request.into_inner()).await?))
	}
}

pub async fn run_server(
	config: Option<GrpcConfig>,
	address: String,
	service: FullNodeService,
	mempool_res_rx: mpsc::Receiver<ResponseMempool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let addr = address.parse().unwrap();
	let cache_capacity = config
		.clone()
		.and_then(|grpc_config| grpc_config.block_cache_capacity)
		.unwrap_or(7200);
	let block_cache: Arc<Cache<BlockNumber, GetBlockV3ByNumberResponse>> = Arc::new(Cache::new(cache_capacity as u64));
	let node_service = FullNodeGrpc { service, mempool_res_rx, block_cache };
	let node_server = NodeServer::new(node_service);
	info!("GRPC server listening on {}", addr);

	let cors = CorsLayer::new()
		.allow_origin(AllowOrigin::any())
		.allow_methods([Method::GET, Method::POST, Method::PUT, Method::HEAD])
		.allow_headers(AllowHeaders::any());

	let grpc_web_layer = GrpcWebLayer::new();

	let mut builder = Server::builder()
		.accept_http1(true)
		.layer(cors)
		.layer(grpc_web_layer);

	if let Some(config) = config {
		if let Some(max_concurrent_streams) = config.max_concurrent_streams {
			builder = builder.max_concurrent_streams(max_concurrent_streams)
		}
		if let Some(concurrency_limit_per_connection) = config.concurrency_limit_per_connection {
			builder = builder.concurrency_limit_per_connection(concurrency_limit_per_connection)
		}
	}

	builder.add_service(node_server)
		.serve(addr)
		.await?;

	Ok(())
}
