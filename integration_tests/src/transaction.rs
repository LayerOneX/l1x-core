use cli::{get_submit_txn_req, types::Transaction};
use l1x_rpc::rpc_model::{
	node_client::NodeClient, GetAccountStateRequest, GetAccountStateResponse, GetChainStateRequest,
	GetCurrentNonceRequest, GetCurrentNonceResponse, SmartContractReadOnlyCallResponse,
	SubmitTransactionResponse,
};
use std::path::PathBuf;
use tonic::transport::Channel;

pub async fn deploy_contract(client: NodeClient<Channel>, private_key: String, account_address: String) {
    let path_to_object_file = PathBuf::from("../fixtures/l1x_test_contract.o");
    let payload = format!("{{\"file\": \"{}\"}}", path_to_object_file.to_str().unwrap());
    let text = "{\"text\": \"00000000000000000000000000000000\"}";
    let tx = Transaction::SmartContractDeployment (
        cli::types::AccessType::PRIVATE,
        cli::types::ContractType::L1XVM,
        cli::types::U8s::Text(payload.to_string()),
        0,
        cli::types::U8s::Text(text.to_string()),
    );

    let answer = submit_tx(client, private_key, tx, account_address).await;
}

pub async fn contract_function_call(function_name: String, args: Vec<String>) {}

// get_counter -> U64
pub async fn read_only_contract_function_call(function_name: String, args: Vec<String>) {}

pub async fn transfer_tokens(from: String, to: String, amount: u32) {}

pub async fn get_account_nonce(account: String) -> u32 {
	0
}

pub async fn get_account_state(account: String) -> () {}

pub async fn submit_tx(
	mut grpc_client: NodeClient<Channel>,
	private_key: String,
	payload: Transaction,
	account_address: String,
) {
	let account_address = l1x_rpc::get_address_from_privkey_str(&private_key).unwrap();
	// let mut grpc_client = grpc_client.unwrap();

	let nonce = {
		let nonce_response = grpc_client
			.get_account_state(GetAccountStateRequest { address: account_address })
			.await
			.unwrap();
		nonce_response.into_inner().account_state.unwrap().nonce.parse().unwrap()
	};

	// let txn: Transaction = serde_json::from_str(payload.as_str()).unwrap();
	let request = get_submit_txn_req(payload, &private_key, 100, nonce).unwrap();
	let response = grpc_client.submit_transaction(request).await.unwrap();
	let result = response.into_inner();
	println!("SubmitTxn = \n{:?}", result);
}
