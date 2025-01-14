use anyhow::Result;
use l1x_rpc::rpc_model::{
	self, transaction, AccountState, CreateStakingPool, EstimateFeeRequest, EstimateFeeResponse, GetAccountStateRequest, GetAccountStateResponse, GetBlockByNumberRequest, GetBlockByNumberResponse, GetChainStateRequest, GetChainStateResponse, GetEventsRequest, GetEventsResponse, GetStakeRequest, GetStakeResponse, GetTransactionReceiptRequest, GetTransactionReceiptResponse, GetTransactionsByAccountRequest, GetTransactionsByAccountResponse, NativeTokenTransfer, SmartContractDeployment, SmartContractFunctionCall, SmartContractInit, Stake, SubmitTransactionRequest, SubmitTransactionResponse, TransactionResponse, UnStake
};

pub fn get_payload_examples() -> Result<String> {
	let address = "0123456789012345678901234567890123456789".to_string();
	let address_bytes = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
	let function_name_bytes = "function_name".as_bytes().to_vec();

	let hash = "0123456789012345678901234567890123456789012345678901234567890123".to_string();
	let veryfying_key = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

	// transactions
	let native_token_transfer =
		NativeTokenTransfer { address: address_bytes.clone(), amount: 100.to_string() };
	let native_token_transfer_str = serde_json::to_string_pretty(&native_token_transfer)?;

	let smart_contract_deployment_str = serde_json::to_string_pretty(&SmartContractDeployment {
		access_type: 0,
		contract_type: 0,
		contract_code: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
		value: 0,
		salt: vec![0; 32],
	})?;

	let smart_contract_init_str = serde_json::to_string_pretty(&SmartContractInit {
		address: address_bytes.clone(),
		arguments: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
	})?;

	let smart_contract_function_call_str =
		serde_json::to_string_pretty(&SmartContractFunctionCall {
			contract_address: address_bytes.clone(),
			function_name: function_name_bytes.clone(),
			arguments: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
		})?;

	let create_staking_pool_str = serde_json::to_string_pretty(&CreateStakingPool {
		contract_instance_address: None,
		min_stake: Some(100.to_string()),
		max_stake: Some(1000.to_string()),
		min_pool_balance: Some(10000.to_string()),
		max_pool_balance: Some(100000.to_string()),
		staking_period: Some(1000000.to_string()),
	})?;

	let stake_str = serde_json::to_string_pretty(&Stake {
		pool_address: address_bytes.clone(),
		amount: 100.to_string(),
	})?;

	let unstake_str = serde_json::to_string_pretty(&UnStake {
		pool_address: address_bytes.clone(),
		amount: 100.to_string(),
	})?;

	//endpoint request/responses
	let get_account_state_request =
		serde_json::to_string_pretty(&GetAccountStateRequest { address: address.clone() })?;
	let get_account_state_response = serde_json::to_string_pretty(&GetAccountStateResponse {
		account_state: Some(AccountState {
			nonce: "1".to_string(),
			balance: "100".to_string(),
			account_type: 0,
		}),
	})?;

	let get_transaction_receipt_request =
		serde_json::to_string_pretty(&GetTransactionReceiptRequest { hash: hash.clone() })?;
	let get_transaction_receipt_response =
		serde_json::to_string_pretty(&GetTransactionReceiptResponse {
			transaction: Some(TransactionResponse {
				transaction: Some(rpc_model::Transaction {
					tx_type: 0,
					transaction: Some(transaction::Transaction::NativeTokenTransfer(
						native_token_transfer.clone(),
					)),
					nonce: "1".to_string(),
					fee_limit: "100".to_string(),
					signature: hex::decode(hash.clone())?,
					verifying_key: veryfying_key.clone(),
				}),
				transaction_hash: hex::decode(
					"98ca6b5e60a8e586e7aef6e3e54fd8da4d54bdd033c9aa11463bb05a6a34c18a",
				)?,
				block_number: 13,
				block_hash: hex::decode(hash.clone())?,
				fee_used: 10.0.to_string(),
				from: hex::decode(address.clone())?,
				timestamp: 100000,
			}),
			status: 0,
		})?;

	let get_transaction_by_account_request =
		serde_json::to_string_pretty(&GetTransactionsByAccountRequest {
			address: address.clone(),
			number_of_transactions: 10,
			starting_from: 100,
		})?;
	let get_transaction_by_account_response =
		serde_json::to_string_pretty(&GetTransactionsByAccountResponse {
			transactions: vec![TransactionResponse {
				transaction: Some(rpc_model::Transaction {
					tx_type: 0,
					transaction: Some(transaction::Transaction::NativeTokenTransfer(
						native_token_transfer.clone(),
					)),
					nonce: "1".to_string(),
					fee_limit: "100".to_string(),
					signature: hex::decode(hash.clone())?,
					verifying_key: veryfying_key.clone(),
				}),
				transaction_hash: hex::decode(
					"98ca6b5e60a8e586e7aef6e3e54fd8da4d54bdd033c9aa11463bb05a6a34c18a",
				)?,
				block_number: 13,
				block_hash: hex::decode(hash.clone())?,
				fee_used: 10.0.to_string(),
				from: hex::decode(address.clone())?,
				timestamp: 100000,
			}],
		})?;
	let get_chain_state_request = serde_json::to_string_pretty(&GetChainStateRequest {})?;
	let get_chain_state_response = serde_json::to_string_pretty(&GetChainStateResponse {
		cluster_address: address.clone(),
		head_block_number: "1".to_string(),
		head_block_hash: hash.clone(),
	})?;

	let get_block_by_number_request =
		serde_json::to_string_pretty(&GetBlockByNumberRequest { block_number: "1".to_string() })?;
	let get_block_by_number_response = serde_json::to_string_pretty(&GetBlockByNumberResponse {
		block: Some(l1x_rpc::rpc_model::Block {
			block_type: 0,
			cluster_address: address.clone(),
			number: "1".to_string(),
			hash: hash.clone(),
			parent_hash: hash.clone(),
			timestamp: 100000,
			transactions: vec![TransactionResponse {
				transaction: Some(rpc_model::Transaction {
					tx_type: 0,
					transaction: Some(transaction::Transaction::NativeTokenTransfer(
						native_token_transfer.clone(),
					)),
					nonce: "1".to_string(),
					fee_limit: "100".to_string(),
					signature: hex::decode(hash.clone())?,
					verifying_key: veryfying_key.clone(),
				}),
				transaction_hash: hex::decode(
					"98ca6b5e60a8e586e7aef6e3e54fd8da4d54bdd033c9aa11463bb05a6a34c18a",
				)?,
				block_number: 13,
				block_hash: hex::decode(hash.clone())?,
				fee_used: 10.0.to_string(),
				from: hex::decode(address.clone())?,
				timestamp: 100000,
			}],
		}),
	})?;

	let get_stake_request = serde_json::to_string_pretty(&GetStakeRequest {
		pool_address: address.clone(),
		account_address: address.clone(),
	})?;
	let get_stake_response =
		serde_json::to_string_pretty(&GetStakeResponse { amount: "100".to_string() })?;

	let get_events_request = serde_json::to_string_pretty(&GetEventsRequest {
		tx_hash: hash.clone(),
		timestamp: 100000,
	})?;
	let get_events_response = serde_json::to_string_pretty(&GetEventsResponse {
		events_data: vec![vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]],
	})?;

	let submit_transaction_request = serde_json::to_string_pretty(&SubmitTransactionRequest {
		nonce: "1".to_string(),
		fee_limit: "100".to_string(),
		signature: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
		verifying_key: veryfying_key.clone(),
		transaction_type: Some(
			l1x_rpc::rpc_model::submit_transaction_request::TransactionType::NativeTokenTransfer(
				native_token_transfer.clone(),
			),
		),
	})?;

	let submit_transaction_response = serde_json::to_string_pretty(&SubmitTransactionResponse {
		hash: hash.clone(),
		contract_address: Some(address.clone()),
	})?;

	let estimate_fee_request = serde_json::to_string_pretty(&EstimateFeeRequest {
		fee_limit: "100".to_string(),
		verifying_key: veryfying_key.clone(),
		transaction_type: Some(
			l1x_rpc::rpc_model::estimate_fee_request::TransactionType::NativeTokenTransfer(
				native_token_transfer,
			),
		),
	})?;

	let estimate_fee_response: String = serde_json::to_string_pretty(&EstimateFeeResponse { fee: 0.to_string() })?;
	
	let get_read_only_call_request =
		serde_json::to_string_pretty(&l1x_rpc::rpc_model::SmartContractReadOnlyCallRequest {
			call: Some(SmartContractFunctionCall {
				contract_address: address_bytes.clone(),
				function_name: function_name_bytes.clone(),
				arguments: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
			}),
		})?;
	let get_read_only_call_response =
		serde_json::to_string_pretty(&l1x_rpc::rpc_model::SmartContractReadOnlyCallResponse {
			status: 0,
			result: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
		})?;

	let get_current_nonce_request =
		serde_json::to_string_pretty(&l1x_rpc::rpc_model::GetCurrentNonceRequest {
			address: address.clone(),
		})?;
	let get_current_nonce_response =
		serde_json::to_string_pretty(&l1x_rpc::rpc_model::GetCurrentNonceResponse {
			nonce: "1".to_string(),
		})?;

	let example_message = format!(
		"-=-=-=-=-=-=-=-=-=-=- Endpoint payloads -=-=-=-=-=-=-=-=-=-=-

l1x_getAccountState:
- request:  {get_account_state_request}
- response: {get_account_state_response}

l1x_getTransactionReceipt:
- request:  {get_transaction_receipt_request}
- response: {get_transaction_receipt_response}

l1x_getTransactionsByAccount:
- request:  {get_transaction_by_account_request}
- response: {get_transaction_by_account_response}

l1x_getChainState:
- request:  {get_chain_state_request}
- response: {get_chain_state_response}

l1x_getBlockByNumber:
- request:  {get_block_by_number_request}
- response: {get_block_by_number_response}

l1x_getStake:
- request:  {get_stake_request}
- response: {get_stake_response}

l1x_getEvents:
- request:  {get_events_request}
- response: {get_events_response}

l1x_getCurrentNonce
- request: {get_current_nonce_request}
- response: {get_current_nonce_response}

l1x_submitTransaction:
- request:  {submit_transaction_request}
- response: {submit_transaction_response}

l1x_estimateFee:
- request:  {estimate_fee_request}
- response: {estimate_fee_response}

l1x_smartContractReadOnlyCall
- request: {get_read_only_call_request}
- response: {get_read_only_call_response}

-=-=-=-=-=-=-=-=-=-=- Transaction examples -=-=-=-=-=-=-=-=-=-=-

NativeTokenTransfer: {native_token_transfer_str}

SmartContractDeployment: {smart_contract_deployment_str}

SmartContractInit: {smart_contract_init_str}

SmartContractFunctionCall: {smart_contract_function_call_str}

CreateStakingPool: {create_staking_pool_str}

Stake: {stake_str}

Unstake: {unstake_str}
"
	);
	Ok(example_message)
}
