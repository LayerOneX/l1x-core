use cli::{get_submit_txn_req, types::Transaction};
use l1x_rpc::rpc_model::{
	node_client::NodeClient, AccountState, GetAccountStateRequest,
	SmartContractReadOnlyCallRequest, SmartContractReadOnlyCallResponse, SubmitTransactionResponse,
};
use tonic::transport::Channel;

#[derive(Clone)]
pub struct Server {
	_endpoint: String,
	client: NodeClient<Channel>,
}

impl Server {
	pub async fn connect(endpoint: String) -> Self {
		let client = NodeClient::connect(endpoint.clone()).await.unwrap();
		Server { _endpoint: endpoint, client }
	}

	pub async fn send_tx(
		&mut self,
		payload: Transaction,
		account_address: String,
		private_key: String,
	) -> SubmitTransactionResponse {
		let nonce: u128 =
			self.get_account_state(account_address.clone()).await.nonce.parse().unwrap();
		let request = get_submit_txn_req(payload.clone(), &private_key, 100, nonce + 1)
			.map_err(|e| {
				println!("Failed to get request from transaction {:?}. Error: {:?}", payload, e);
				e
			})
			.unwrap();
		let response = self.client.submit_transaction(request).await.unwrap();
		let mut result = response.into_inner();
		result.message().await.unwrap().unwrap()
	}

	pub async fn send_readonly_tx(
		&mut self,
		payload: SmartContractReadOnlyCallRequest,
	) -> SmartContractReadOnlyCallResponse {
		self.client.smart_contract_read_only_call(payload).await.unwrap().into_inner()
	}

	pub async fn get_account_state(&mut self, address: String) -> AccountState {
		let account_state =
			self.client.get_account_state(GetAccountStateRequest { address }).await.unwrap();
		let account_state = account_state.into_inner().account_state.unwrap();
		account_state
	}
}
