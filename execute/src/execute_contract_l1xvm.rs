use crate::{consts, execute_common::ContractExecutor, execute_token::ExecuteToken};
use anyhow::{anyhow, Error};
use async_trait::async_trait;
use contract_instance_manager::contract_instance_manager::ContractInstanceManager;
use evm_interpreter::{config::Config, GasometerState};
use evm_runtime::{Capture, Context, CreateScheme, ExitError, ExitReason, ExitSucceed};
use l1x_ebpf_runtime::run;
use l1x_evm::tagged_runtime::RuntimeInterrupt;
use log::info;
use primitive_types::{H160, H256, U256};
use primitives::*;
use rand::Rng;
use sha3::{Digest, Keccak256};
use state::UpdatedState;
use std::{borrow::Cow, cell::RefCell, rc::Rc, sync::Arc};
use system::{
	account::Account,
	contract::{Contract, ContractType},
	contract_instance::ContractInstance,
	event::{Event, EventType},
	network::EventBroadcast,
};
use tokio::sync::broadcast;
use traits::l1xvm_contract_call::L1XVMContractCallTrait;
use vm_execution_fee::execution_fees::VmExecutionFees;
const CONTRACT_INIT_FUNCTION_NAME: &str = "new";

pub struct ExecuteContract;

#[async_trait]
impl<'a, 'g> ContractExecutor<'a, 'g> for ExecuteContract {
	#[allow(unused_variables)]
	fn execute_contract_deployment(
		&self,
		account_address: &Address,
		scheme: CreateScheme,
		context: Context,
		cluster_address: &Address,
		access_type: AccessType,
		contract_code: &ContractCode,
		nonce: Nonce,
		gas_limit: Gas,
		_deposit: Balance,
		transaction_hash: &TransactionHash,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> Result<(Address, Gas), Error> {
		let contract_address = Account::contract_address(account_address, cluster_address, nonce);
		
		updated_state.create_system_account(&contract_address)?;
		
		let contract = Contract {
			address: contract_address.clone(),
			access: access_type,
			code: contract_code.clone(),
			r#type: ContractType::L1XVM as i8,
			owner_address: account_address.clone(),
		};

		updated_state.store_contract(&contract)?;

		let message = format!(
			r#"
            +---------------------------------------------------------+
            ☀️ EXECUTING CONTRACT DEPLOYMENT ☀️ ➡️ Contract Address: {}
            +---------------------------------------------------------+
            "#,
			hex::encode(contract_address)
		);

		info!("{}", message);

		let event_data = "Contract deployment succeeded";
		let event = Event::new(
			transaction_hash.clone(),
			event_data.as_bytes().to_vec(),
			block_number,
			EventType::L1XVM as i8,
			contract_address,
			None,
		);

		updated_state.create_event(&event)?;
		
		let burnt_gas = 0;
		Ok((contract_address, burnt_gas))
	}

	fn execute_contract_init(
		&self,
		account_address: &Address,
		context: Context,
		gas_limit: Gas,
		deposit: Balance,
		cluster_address: &Address,
		contract: &Contract,
		arguments: ContractArgument,
		nonce: Nonce,
		transaction_hash: &TransactionHash,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> Result<(ContractInstance, Gas), Error> {
		let contract_instance_address = Account::contract_instance_address(
			account_address,
			&contract.address,
			cluster_address,
			nonce,
		);
		info!(
			"*******EXECUTING CONTRACT INIT******** : {:?}",
			hex::encode(contract_instance_address)
		);

		updated_state.create_system_account(&contract_instance_address)?;

		let contract_instance = ContractInstance {
			instance_address: contract_instance_address.clone(),
			contract_address: contract.address,
			owner_address: account_address.clone(),
		};
		
		updated_state.store_contract_instance(&contract_instance)?;

		let (ret, burnt_gas) = self
			.execute_contract_function_call(
				&account_address,
				context,
				gas_limit,
				deposit,
				cluster_address,
				&contract,
				&contract_instance,
				&CONTRACT_INIT_FUNCTION_NAME.as_bytes().to_vec(), /* Init function should be
				                                                   * hard coded */
				&arguments,
				nonce,
				&transaction_hash,
				false,
				block_number,
				block_hash,
				block_timestamp,
				current_call_depth,
				updated_state,
				event_tx.clone(),
			);

		match ret {
			Capture::Exit(r) => {
				let (reason, _) = r;
				if !matches!(reason, ExitReason::Succeed(ExitSucceed::Returned)) {
					Err(anyhow!("{:?}", reason))
				} else {
					Ok(())
				}
			},
			Capture::Trap(_) => Err(anyhow!("Error: trap")),
		}?;

		Ok((contract_instance, burnt_gas))
	}

	fn execute_contract_function_call(
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
		readonly: bool,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> (Capture<(ExitReason, Arc<Vec<u8>>), RuntimeInterrupt>, Gas) {
		if current_call_depth > consts::MAX_CROSS_CONTRACT_CALL_DEPTH {
			return (
				Capture::Exit((
					ExitReason::Error(ExitError::Other(Cow::Owned(format!(
						"Max cross-contarct call depth reached: limit={}",
						consts::MAX_CROSS_CONTRACT_CALL_DEPTH
					)))),
					Arc::new(vec![]),
				)),
				0,
			);
		}

		let contract_instance_address = contract_instance.instance_address.clone();

		if !readonly && deposit > 0 {
			// Transfer "deposit" to the contract instance_address
			if let Err(e) = ExecuteToken::just_transfer_tokens(account_address, &contract_instance_address, deposit, updated_state) {
				return (
					Capture::Exit((
						ExitReason::Error(ExitError::Other(Cow::Owned(format!(
							"Can't transfer deposit: amount={}, from={}, to={}, err={:?}",
							deposit, hex::encode(&account_address), hex::encode(&contract_instance_address), e
						)))),
						Arc::new(vec![]),
					)),
					0,
				);
			}
		}

		let function = function.clone();
		let arguments = arguments.clone();
		let caller_address = account_address.clone();
		let cluster_address = cluster_address.clone();
		let contract_code_address = contract_instance.contract_address.clone();
		let contract_code = contract.code.clone();
		let transaction_hash = transaction_hash.clone();
		let block_hash = block_hash.clone();
		let owner_address = contract_instance.owner_address.clone();

		let contract_instance = contract_instance.clone();
		let config = Arc::new(Config::shanghai());
		let gasometer = Rc::new(RefCell::new(GasometerState::new(gas_limit, readonly, &config)));
		let vm_ret = {
			let evm =
				Arc::new(RefCell::new(crate::execute_contract_evm::ExecuteContract {}));
			let l1xvm = Arc::new(RefCell::new(ExecuteContract {}));
			let l1xvm_stake =
				Arc::new(RefCell::new(crate::execute_staking::ExecuteStaking {}));
			let mut contract_instance_manager = match ContractInstanceManager::new(
				Some(contract_instance),
				cluster_address,
				caller_address.clone(),
				nonce,
				transaction_hash,
				block_number,
				block_hash,
				block_timestamp,
				context.clone(),
				readonly,
				evm,
				l1xvm,
				l1xvm_stake,
				current_call_depth,
				event_tx,
				updated_state,
				gasometer,
			) {
				Ok(cim) => cim,
				Err(e) => {
					return (
						Capture::Exit((
							ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", e)))),
							e.to_string().into_bytes().into(),
						)),
						0,
					);
				},
			};

			let fee_config = VmExecutionFees::create_basic_config();
			//If possible return Result<EventData, Error> instead of Result<String, Error>
			let ret = run(
				&contract_code,
				function.clone(),
				arguments.clone(),
				caller_address.clone(),
				contract_code_address.clone(),
				contract_instance_address.clone(),
				owner_address.clone(),
				block_number,
				block_hash,
				block_timestamp,
				readonly,
				gas_limit,
				deposit,
				fee_config,
				Arc::new(RefCell::new(&mut contract_instance_manager)),
			);
			match ret {
				Ok(vm_ret) => {
					updated_state.merge(&contract_instance_manager.updated_state);
					vm_ret
				},
				Err(e) => {
					return (
						Capture::Exit((
							ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", e)))),
							e.to_string().into_bytes().into(),
						)),
						0,
					);
				}
			}
		};
		info!("VM RESULT: {:?}", vm_ret);

		let event_data: EventData = vm_ret.result.into_bytes();
		let burnt_gas = vm_ret.burnt_gas;

		if !readonly {
			let event = Event::new(
				transaction_hash.clone(),
				event_data.clone(),
				block_number,
				EventType::L1XVM as i8,
				contract_instance_address,
				None,
			);
			match updated_state.create_event(&event) {
				Ok(v) => v,
				Err(e) => {
					return (
						Capture::Exit((
							ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", e)))),
							e.to_string().into_bytes().into(),
						)),
						burnt_gas,
					);
				},
			};
	}
		(Capture::Exit((ExitReason::Succeed(ExitSucceed::Returned), event_data.into())), burnt_gas)
	}

	#[allow(unused_variables)]
	fn execute_contract_function_call_read_only(
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
		let context = Context {
			// Execution address.
			address: H160([0u8; 20]), // Does not affect pool id
			// Caller of the EVM.
			caller: H160([0u8; 20]), // Does not affect pool id
			// Apparent value of the EVM.
			apparent_value: U256::zero(),
		};
		let account_address = [0u8; 20];
		let transaction_hash = [0u8; 32];
		let readonly = true;
		let deposit = 0;
		ExecuteContract
			.execute_contract_function_call(
				&account_address,
				context,
				gas_limit,
				deposit,
				&cluster_address,
				&contract,
				contract_instance,
				&function,
				&arguments,
				nonce,
				&transaction_hash,
				readonly,
				block_number,
				block_hash,
				block_timestamp,
				current_call_depth,
				updated_state,
				event_tx,
			)
	}
}

impl<'a> L1XVMContractCallTrait<'a> for ExecuteContract {
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
	) -> Result<(Address, Gas), Error> {
		let caller = account_address;
		let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
		let salt = generate_salt();
		let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
		let context = Context {
			// Execution address.
			address: H160(*account_address), // Does not affect pool id
			// Caller of the EVM.
			caller: H160(*account_address), // Does not affect pool id
			// Apparent value of the EVM.
			apparent_value: U256::zero(),
		};
		let deposit = 0;
		self.execute_contract_deployment(
			account_address,
			scheme,
			context,
			cluster_address,
			access_type,
			contract_code,
			nonce,
			0,
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

	fn execute_l1xvm_contract_init(
		&self,
		_account_address: &Address,
		_cluster_address: &Address,
		_access_type: AccessType,
		_contract_code: &ContractCode,
		_nonce: Nonce,
		_transaction_hash: &TransactionHash,
		_gas_limit: Gas,
		_deposit: Balance,
		_block_number: BlockNumber,
		_block_hash: &BlockHash,
		_block_timestamp: BlockTimeStamp,
		_current_call_depth: u32,
		_updated_state: &mut UpdatedState,
	) -> Result<Address, Error> {
		Ok([0u8; 20])
	}

	fn execute_l1xvm_contract_call(
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
		let context = Context {
			// Execution address.
			address: H160(*account_address), // Does not affect pool id
			// Caller of the EVM.
			caller: H160(*account_address), // Does not affect pool id
			// Apparent value of the EVM.
			apparent_value: U256::zero(),
		};
		self.execute_contract_function_call(
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
}

pub fn generate_salt() -> H256 {
	let mut rng = rand::thread_rng();
	let mut salt: H256 = H256::default();

	for byte in salt.0.iter_mut() {
		*byte = rng.gen();
	}
	salt
}
