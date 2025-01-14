use anyhow::Error;
use evm_interpreter::GasometerState;
use evm_runtime::{Capture, Context, CreateScheme, ExitReason};
use l1x_evm::{evm_call::EVMContractCallTrait, tagged_runtime::RuntimeInterrupt};
use primitives::*;
use state::UpdatedState;
use std::{
	cell::RefCell, rc::Rc, sync::Arc
};
use system::{contract::Contract, contract_instance::ContractInstance, network::EventBroadcast};
use tokio::sync::broadcast;
use traits::{l1xvm_contract_call::L1XVMContractCallTrait, l1xvm_stake_call::L1XVMStakeCallTrait};

pub struct ContractInstanceManager<'a, 'g> {
	pub contract_instance: Option<ContractInstance>,
	pub cluster_address: Address,
	pub account_address: Address,
	pub nonce: Nonce,
	pub transaction_hash: TransactionHash,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
	pub block_timestamp: BlockTimeStamp,
	pub context: Context,
	pub readonly: bool,
	pub evm: EVMSyncApi<'a, 'g>,
	pub l1xvm: L1XVMSyncApi<'a>,
	pub l1xvm_stake: L1XVMStakeSyncApi<'a>,
	pub current_call_depth: u32,
	// pub rt: tokio::runtime::Runtime,
	pub updated_state: UpdatedState,
	pub event_tx: broadcast::Sender<EventBroadcast>,
}

impl<'a, 'g> ContractInstanceManager<'a, 'g> {
	pub fn new(
		contract_instance: Option<ContractInstance>,
		cluster_address: Address,
		account_address: Address,
		nonce: Nonce,
		transaction_hash: TransactionHash,
		block_number: BlockNumber,
		block_hash: BlockHash,
		block_timestamp: BlockTimeStamp,
		context: Context,
		readonly: bool,
		evm: Arc<RefCell<dyn EVMContractCallTrait<'a, 'g>>>,
		l1xvm: Arc<RefCell<dyn L1XVMContractCallTrait<'a>>>,
		l1xvm_stake: Arc<RefCell<dyn L1XVMStakeCallTrait<'a>>>,
		current_call_depth: u32,
		event_tx: broadcast::Sender<EventBroadcast>,
		updated_state: &UpdatedState,
		gasometer: Rc<RefCell<GasometerState<'g>>>,
	) -> Result<ContractInstanceManager<'a, 'g>, Error> {
		// let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
		let manager = ContractInstanceManager {
			contract_instance,
			cluster_address,
			account_address,
			nonce,
			transaction_hash,
			block_number,
			block_hash,
			block_timestamp,
			context,
			readonly,
			evm: EVMSyncApi::new(evm, gasometer),
			l1xvm: L1XVMSyncApi::new(l1xvm),
			l1xvm_stake: L1XVMStakeSyncApi::new(l1xvm_stake),
			current_call_depth,
			// rt,
			updated_state: updated_state.clone(),
			event_tx,
		};
		Ok(manager)
	}
}

pub struct EVMSyncApi<'a, 'g> {
	pub evm: Arc<RefCell<dyn EVMContractCallTrait<'a, 'g>>>,
	pub gasometer: Rc<RefCell<GasometerState<'g>>>,
}

impl<'a, 'g> EVMSyncApi<'a, 'g> {
	pub fn new(evm: Arc<RefCell<dyn EVMContractCallTrait<'a, 'g>>>, gasometer: Rc<RefCell<GasometerState<'g>>>) -> Self {
		Self { evm, gasometer }
	}

	pub fn execute_contract_deployment(
		&self,
		account_address: &Address,
		scheme: CreateScheme,
		context: Context,
		gas_limit: Gas,
		deposit: Balance,
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
	) -> Result<(Address, Gas), Error> {
		let evm = self.evm.borrow();
		evm.execute_evm_contract_deployment(
			account_address,
			scheme,
			context,
			cluster_address,
			access_type,
			contract_code,
			nonce,
			gas_limit,
			deposit,
			transaction_hash,
			block_number,
			block_hash,
			block_timestamp,
			current_call_depth,
			updated_state,
			event_tx,
		)
	}

	pub fn execute_contract_function_call(
		&self,
		account_address: &Address,
		context: Context,
		gas_limit: Gas,
		deposit: Balance,
		cluster_address: &Address,
		contract: &Contract,
		contract_instance: &ContractInstance,
		function: &ContractFunction,
		arguments: &ContractArgument,
		nonce: Nonce,
		transaction_hash: &TransactionHash,
		is_read_only: bool,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> (Capture<(ExitReason, Arc<Vec<u8>>), RuntimeInterrupt>, Gas) {
		let evm = self.evm.borrow();
		evm.execute_evm_contract_call(
			account_address,
			context,
			gas_limit,
			deposit,
			cluster_address,
			contract,
			contract_instance,
			function,
			arguments,
			nonce,
			transaction_hash,
			is_read_only,
			block_number,
			block_hash,
			block_timestamp,
			current_call_depth,
			updated_state,
			event_tx,
		)
	}

	pub fn execute_evm_function_call_readonly(
		&self,
		contract: &Contract,
		contract_instance: &ContractInstance,
		function: &ContractFunction,
		arguments: &ContractArgument,
		gas_limit: Gas,
		cluster_address: &Address,
		nonce: Nonce,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> (Capture<(ExitReason, Arc<Vec<u8>>), RuntimeInterrupt>, Gas) {
		let evm = self.evm.borrow();
		evm.execute_evm_function_call_readonly(
			contract,
			contract_instance,
			function,
			arguments,
			gas_limit,
			cluster_address,
			nonce,
			block_number,
			block_hash,
			block_timestamp,
			current_call_depth,
			updated_state,
			event_tx,
		)
	}
}

pub struct L1XVMSyncApi<'a> {
	pub l1xvm: Arc<RefCell<dyn L1XVMContractCallTrait<'a>>>,
}

impl<'a> L1XVMSyncApi<'a> {
	pub fn new(l1xvm: Arc<RefCell<dyn L1XVMContractCallTrait<'a>>>) -> Self {
		Self { l1xvm }
	}

	pub fn execute_contract_deployment(
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
	) -> Result<(Address, Gas), Error> {
		let l1xvm = self.l1xvm.borrow();
		l1xvm.execute_l1xvm_contract_deployment(
			account_address,
			cluster_address,
			access_type,
			contract_code,
			nonce,
			transaction_hash,
			block_number,
			block_hash,
			block_timestamp,
			current_call_depth,
			updated_state,
			event_tx,
		)
	}

	#[allow(dead_code)]
	fn execute_l1xvm_contract_init(
		&self,
		_account_address: &Address,
		_cluster_address: &Address,
		_access_type: AccessType,
		_contract_code: &ContractCode,
		_nonce: Nonce,
		_transaction_hash: &TransactionHash,
		_block_number: BlockNumber,
		_block_hash: &BlockHash,
		_block_timestamp: BlockTimeStamp,
	) -> Result<Address, Error> {
		Ok([0u8; 20])
	}

	pub fn execute_contract_function_call(
		&self,
		account_address: &Address,
		cluster_address: &Address,
		contract: &Contract,
		contract_instance: &ContractInstance,
		function: &ContractFunction,
		arguments: &ContractArgument,
		gas_limit: Gas,
		deposit: Balance,
		nonce: Nonce,
		transaction_hash: &TransactionHash,
		is_read_only: bool,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> (Capture<(ExitReason, Arc<Vec<u8>>), RuntimeInterrupt>, Gas) {
		let l1xvm = self.l1xvm.borrow();
		l1xvm.execute_l1xvm_contract_call(
			account_address,
			cluster_address,
			contract,
			contract_instance,
			function,
			arguments,
			gas_limit,
			deposit,
			nonce,
			transaction_hash,
			is_read_only,
			block_number,
			block_hash,
			block_timestamp,
			current_call_depth,
			updated_state,
			event_tx,
		)
	}
}

pub struct L1XVMStakeSyncApi<'a> {
	pub l1xvm_stake: Arc<RefCell<dyn L1XVMStakeCallTrait<'a>>>,
}

impl<'a> L1XVMStakeSyncApi<'a> {
	pub fn new(l1xvm_stake: Arc<RefCell<dyn L1XVMStakeCallTrait<'a>>>) -> Self {
		Self { l1xvm_stake }
	}
	pub fn execute_native_staking_create_pool(
		&self,
		account_address: &Address,
		cluster_address: &Address,
		nonce: Nonce,
		created_block_number: BlockNumber,
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
		updated_state: &mut UpdatedState,
	) -> Result<Address, Error> {
		let l1xvm_stake = self.l1xvm_stake.borrow();
		l1xvm_stake.execute_native_staking_create_pool_call(
			account_address,
			cluster_address,
			nonce,
			created_block_number,
			contract_instance_address,
			min_stake,
			max_stake,
			min_pool_balance,
			max_pool_balance,
			staking_period,
			updated_state,
		)
	}

	pub fn execute_native_staking_stake(
		&self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error> {
		let l1xvm_stake = self.l1xvm_stake.borrow();
		l1xvm_stake.execute_native_staking_stake_call(
			pool_address,
			account_address,
			block_number,
			amount,
			updated_state,
		)
	}

	pub fn execute_native_staking_un_stake(
		&self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error> {
		let l1xvm_stake = self.l1xvm_stake.borrow();
		l1xvm_stake.execute_native_staking_un_stake_call(
			pool_address,
			account_address,
			block_number,
			amount,
			updated_state,
		)
	}
}
