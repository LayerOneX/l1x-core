use anyhow::Error;
use evm_runtime::{Capture, ExitReason};
use l1x_evm::tagged_runtime::RuntimeInterrupt;
use primitives::*;
use state::UpdatedState;
use std::sync::Arc;
use system::{contract::Contract, contract_instance::ContractInstance, network::EventBroadcast};
use tokio::sync::broadcast;

pub trait L1XVMContractCallTrait<'a> {
	fn execute_l1xvm_contract_deployment(
		&self,
		account_address: &Address,
		cluster_address: &Address,
		access_type: AccessType,
		contract_code: &ContractCode,
		nonce: Nonce,
		transaction_hash: &TransactionHash,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> Result<(Address, Gas), Error>;

	fn execute_l1xvm_contract_init(
		&self,
		account_address: &Address,
		cluster_address: &Address,
		access_type: AccessType,
		contract_code: &ContractCode,
		nonce: Nonce,
		transaction_hash: &TransactionHash,
		gas_limit: Gas,
		despoit: Balance,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
	) -> Result<Address, Error>;

	fn execute_l1xvm_contract_call(
		&self,
		account_address: &Address,
		cluster_address: &Address,
		contract: &Contract,
		contract_instance: &ContractInstance,
		function: &ContractFunction,
		arguments: &ContractArgument,
		gas_limit: Gas,
		despoit: Balance,
		nonce: Nonce,
		transaction_hash: &TransactionHash,
		is_read_only: bool,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> (Capture<(ExitReason, Arc<Vec<u8>>), RuntimeInterrupt>, Gas);
}
