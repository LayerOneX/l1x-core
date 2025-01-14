use crate::{consts, execute_common::ContractExecutor, execute_token::ExecuteToken};
use state::UpdatedState;
use anyhow::{Error, anyhow};
use async_trait::async_trait;
use contract_instance_manager::contract_instance_manager::ContractInstanceManager;
use evm::maybe_borrowed::MaybeBorrowed;
use evm_interpreter::{config::Config, GasometerState, TransactionCost};
use evm_runtime::{
	Capture, Context, CreateScheme, ExitError, ExitReason, ExitSucceed, Opcode, Resolve, Runtime,
};
use l1x_evm::{
	evm_call::EVMContractCallTrait,
	tagged_runtime::{create_evm_address, RuntimeInterrupt, RuntimeKind, TaggedRuntime},
};
use log::{error, info};
use primitive_types::{H160, U256};
use primitives::*;
use std::{
	borrow::Cow,
	cell::RefCell,
	rc::Rc,
	sync::Arc,
};
use system::{
	account::Account,
	contract::{Contract, ContractType},
	contract_instance::ContractInstance,
	event::{Event, EventType},
	network::EventBroadcast,
};
use tokio::sync::broadcast;

pub struct ExecuteContract;

impl<'a, 'g> ExecuteContract {
	fn verify_code(config: &Config, code: &[u8]) -> Result<(), ExitError> {
		// As of EIP-3541 code starting with 0xef cannot be deployed
		if config.disallow_executable_format && Some(&Opcode::EOFMAGIC.as_u8()) == code.first()
		{
			return Err(ExitError::InvalidCode(Opcode::EOFMAGIC))
		}

		
		if let Some(limit) = config.create_contract_limit {
			// `limit` is multiplied by 2 to fix https://github.com/L1X-Foundation-Consensus/l1x-consensus/issues/961
			if code.len() > limit * 2 {
				return Err(ExitError::CreateContractLimit)
			}
		}
		Ok(())
	}

	fn cleanup_for_create(
		config: &Config,
		created_address: H160,
		reason: ExitReason,
		return_data: Vec<u8>,
	) -> (ExitReason, Option<H160>, (Result<(), Error>, Vec<u8>)) {
		log::debug!(target: "evm", "Create execution using address {}: {:?}", created_address, reason);

		match reason {
			ExitReason::Succeed(ret) => {;
				if let Err(e) = Self::verify_code(config, &return_data) {
					(e.clone().into(), None, (Err(anyhow!("{:?}", e)), return_data))
				} else {
					(ret.into(), Some(created_address), (Ok(()), return_data))
				}
			}
			ExitReason::Error(e) => (e.clone().into(), None, (Err(anyhow!("{:?}", e)), return_data)),
			ExitReason::Revert(e) => (e.clone().into(), None, (Err(anyhow!("{:?}", e)), return_data)),
			ExitReason::Fatal(e) => (e.clone().into(), None, (Err(anyhow!("{:?}", e)), return_data)),
		}
	}

	fn cleanup_for_call(
		reason: &ExitReason,
		return_data: Vec<u8>,
	) -> (anyhow::Result<()>, Vec<u8>) {
		match reason {
			ExitReason::Succeed(s) => {
				info!("cleanup_for_call: ExitReason::Succeed: {:?}", s);
				(Ok(()), return_data)
			},
			ExitReason::Error(e) => {
				info!("cleanup_for_call: ExitReason::Error: {:?}", e);
				(Err(anyhow::anyhow!("{:?}", e)), return_data)
			},
			ExitReason::Revert(e) => {
				info!("cleanup_for_call: ExitReason::Revert: {:?}", e);
				(Err(anyhow::anyhow!("{:?}", e)), return_data)
			},
			ExitReason::Fatal(e) => {
				info!("cleanup_for_call: ExitReason::Fatal: {:?}", e);
				(Err(anyhow::anyhow!("{:?}", e)), return_data)
			},
		}
	}

	pub fn execute_evm_runtime(
		&self,
		account_address: &Address,
		cluster_address: &Address,
		access_type: AccessType,
		contract_code: &ContractCode,
		arguments: &ContractArgument,
		nonce: Nonce,
		transaction_hash: &TransactionHash,
		gas_limit: Gas,
		is_read_only: bool,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		runtime_kind: RuntimeKind,
		context: Context,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> (Capture<(ExitReason, Arc<Vec<u8>>), RuntimeInterrupt>, Gas) {
		let config = Arc::new(Config::shanghai());
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
		info!("**** BEGINGS execute_evm_runtime from={} caller={:?} to={:?} read_only={} ****",
			hex::encode(&account_address), &context.caller, &context.address, is_read_only);
		let arguments = arguments.clone();
		let account_address = account_address.clone();
		let cluster_address = cluster_address.clone();
		let access_type = access_type.clone();
		let contract_code = contract_code.clone();
		let transaction_hash = transaction_hash.clone();
		let block_hash = block_hash.clone();
		let gasometer = Rc::new(RefCell::new(GasometerState::new(gas_limit, is_read_only, &config)));

		let execution_ret: Result<(RuntimeKind, Vec<u8>, ExitReason), Error> = {
			let evm = Arc::new(RefCell::new(ExecuteContract {}));
			let l1xvm =
				Arc::new(RefCell::new(crate::execute_contract_l1xvm::ExecuteContract {}));
			let l1xvm_stake =
				Arc::new(RefCell::new(crate::execute_staking::ExecuteStaking {}));
			let mut contract_instance_manager = match ContractInstanceManager::new(
				None,
				cluster_address,
				account_address,
				nonce,
				transaction_hash,
				block_number,
				block_hash,
				block_timestamp,
				context.clone(),
				is_read_only,
				evm,
				l1xvm,
				l1xvm_stake,
				current_call_depth,
				event_tx,
				updated_state,
				gasometer.clone(),
			) {
				Ok(cim) => cim,
				Err(e) => {
					return (
						Capture::Exit((
							ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", e)))),
							format!("{:?}", e).into_bytes().into(),
						)),
						0,
					)
				},
			};
			let runtime_inner = Runtime::new(
				Rc::new(contract_code.clone()),
				Rc::new(arguments.clone()),
				context.clone(),
				1024,
				1000000, // memory_limit 1MB because of https://github.com/L1X-Foundation-Consensus/l1x-consensus/issues/961
			);
			let mut runtime = TaggedRuntime {
				kind: runtime_kind,
				inner: MaybeBorrowed::Owned(runtime_inner),
			};
			let reason = {
				let inner_runtime = &mut runtime.inner;
				println!("Before evm run");
				match inner_runtime.run(&mut contract_instance_manager) {
					Capture::Exit(reason) => {
						info!("inner_runtime.run: Capture::Exit(reason): {:?}", reason);
						reason
					},
					Capture::Trap(resolve) => match resolve {
						Resolve::Call(_interrupt, _resolve) => {
							error!("inner_runtime.run: Resolve::Call(interrupt, resolve)");
							// Handle the Call resolve trap if needed
							ExitReason::Error(ExitError::Other(Cow::Owned(
								"inner_runtime.run: Resolve::Call(interrupt, resolve)"
									.to_string(),
							)))
						},
						Resolve::Create(_interrupt, _resolve) => {
							error!(
								"inner_runtime.run: Resolve::Create(interrupt, resolve)"
							);
							// Handle the Create resolve trap if needed
							ExitReason::Error(ExitError::Other(Cow::Owned(
								"inner_runtime.run: Resolve::Create(interrupt, resolve)"
									.to_string(),
							)))
						},
					},
				}
			};
			let runtime_kind = runtime.kind;
			let (reason, maybe_address, (result, return_data)) = match runtime_kind {
				RuntimeKind::Create(created_address) => {
					let gasometer_config = match contract_instance_manager.evm.gasometer.try_borrow_mut() {
							Ok(mut borrowed_gasometer) => {
								let transaction_cost =
									TransactionCost::create(&contract_code, &[])
										.cost(borrowed_gasometer.config);
								match borrowed_gasometer.record_gas64(transaction_cost) {
									Ok(_) => {},
									Err(e) => {
										error!("Error recording gas: {:?}", e);
									},
								}
								Some(borrowed_gasometer.config.clone())
							},
							Err(e) => {
								// Handle the error, e.g., by logging or returning an error
								error!("Failed to acquire lock on gasometer: {:?}", e);
								None
							},
						};
					let config = match gasometer_config{
						Some(config) => config,
						None => {
							let msg = "gasometer_config is None";
							return (
								Capture::Exit((
									ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", msg)))),
									format!("{:?}", msg).into_bytes().into(),
								)),
								0,
							)
						},
					};
					let (reason, maybe_address, result) = Self::cleanup_for_create(
						&config,
						created_address.clone(),
						reason.clone(),
						runtime.inner.machine().return_value(),
					);

					(reason, maybe_address, result)
				},
				RuntimeKind::Call(_code_address) => {
					let gasometer_config = match contract_instance_manager.evm.gasometer.try_borrow_mut() {
							Ok(mut locked_gasometer) => {
								let transaction_cost =
									TransactionCost::call(&arguments, &[])
										.cost(locked_gasometer.config);

								match locked_gasometer.record_gas64(transaction_cost) {
									Ok(_) => {},
									Err(e) => {
										error!("Error recording gas: {:?}", e);
									},
								}
								Some(locked_gasometer.config.clone())
							},
							Err(e) => {
								// Handle the error, e.g., by logging or returning an error
								log::error!("Failed to acquire lock on gasometer: {:?}", e);
								None
							},
					};
					match gasometer_config {
						Some(_config) => (),
						None => {
							let msg = "gasometer_config is None 2";
							return (
								Capture::Exit((
									ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", msg)))),
									format!("{:?}", msg).into_bytes().into(),
								)),
								0,
							)
						},
					};
					let return_data = Self::cleanup_for_call(
						&reason,
						runtime.inner.machine().return_value(),
					);
					(reason, None, return_data)
				},
			};

			match result {
				Ok(_) => {
					let inner_runtime = &mut runtime.inner;
					let maybe_error = match runtime_kind {
						RuntimeKind::Create(_) =>
							inner_runtime.finish_create(reason, maybe_address, return_data.clone()),
						RuntimeKind::Call(_) =>
							inner_runtime.finish_call(reason, return_data.clone()),
					};
		
					// Early exit if passing on the result caused an error
					info!("cleanup_for_call, maybe_error: {:?}", maybe_error);
					if let Err(e) = maybe_error {
						Ok((
							runtime_kind,
							return_data.clone(),
							ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", e)))),
						))
					} else {
						updated_state.merge(&contract_instance_manager.updated_state);
						Ok((
							runtime_kind,
							return_data.clone(),
							ExitReason::Succeed(ExitSucceed::Returned),
						))
					}
				}
				Err(e) => {
					info!("cleanup_for_call, return_data error {:?}", e);
					Ok((
						runtime_kind,
						return_data.clone(),
						ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", e)))),
					))
				},
			}
		};
		
		let burnt_gas = gasometer.try_borrow().map(|meter| {
			let total_used_gas = meter.total_used_gas();
			let refunded_gas = meter.get_refunded_gas(); // Using the new method
			total_used_gas.saturating_sub(refunded_gas)
		}).unwrap_or_else(|e| {
			log::error!("Failed to lock gasometer: {:?}", e);
			0u64 // Default to 0 if unable to acquire the lock.
		});		
		let vm_result = match execution_ret {
			Ok(r) => r,
			Err(e) =>
				return (
					Capture::Exit((
						ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", e)))),
						format!("{:?}", e).into_bytes().into(),
					)),
					burnt_gas,
				),
		};
	
		let (runtime_kind, return_data, exit_reason) = vm_result;
		match runtime_kind {
			RuntimeKind::Create(contract_instance_address) => {
				let contract_code_runtime = return_data.clone();
				if !is_read_only && exit_reason.is_succeed() {
					match Self::store_evm_contract(
						&account_address,
						&cluster_address,
						&contract_instance_address.into(),
						access_type,
						nonce,
						&contract_code_runtime,
						&transaction_hash,
						block_number,
						updated_state,
					)
					{
						Ok(contract_instance_address) => {
							info!(
								"*********** EVM DEPLOYED CONTRACT ADDRESS {:?}",
								hex::encode(contract_instance_address.clone())
							);
							//contract_instance_address
						},
						Err(e) => {
							return (
								Capture::Exit((
									ExitReason::Error(ExitError::Other(Cow::Owned(format!("{:?}", e)))),
									format!("{:?}", e).into_bytes().into(),
								)),
								burnt_gas,
							);
						},
					}
				}
			},
			RuntimeKind::Call(code_address) => {
				info!("RuntimeKind::Call: code_address: {:?}", code_address);
				if !is_read_only {
					let event = Event::new(
						transaction_hash.clone(),
						return_data.clone(),
						block_number,
						EventType::EVM as i8,
						account_address,
						None,
					);
					info!("RuntimeKind::Call: {}", &event);
					match updated_state.create_event(&event) {
						Ok(_) => (),
						Err(e) => {
							return (
								Capture::Exit((
									ExitReason::Error(ExitError::Other(Cow::Owned(format!(
										"{:?}",
										e
									)))),
									format!("{:?}", e).into_bytes().into(),
								)),
								burnt_gas,
							);
						},
					}
				}
			},
		};
		info!("*********** EVM RESULT: {:?}", hex::encode(&return_data));
		// info!("EVM used gas: {:?}", used_gas);
		(Capture::Exit((exit_reason, return_data.into())), burnt_gas)
	}

	fn store_evm_contract(
		account_address: &Address,
		_cluster_address: &Address,
		contract_instance_address: &Address,
		access_type: AccessType,
		_nonce: Nonce,
		contract_code: &ContractCode,
		transaction_hash: &TransactionHash,
		block_number: BlockNumber,
		updated_state: &mut UpdatedState,
	) -> Result<EventData, Error> {
		let contract = Contract {
			address: contract_instance_address.clone(),
			access: access_type.clone(),
			code: contract_code.clone(),
			r#type: ContractType::EVM as i8,
			owner_address: account_address.clone(),
		};
		updated_state.store_contract(&contract)?;

		let contract_instance = ContractInstance {
			instance_address: contract_instance_address.clone(),
			contract_address: contract.address,
			owner_address: account_address.clone(),
		};
		updated_state.store_contract_instance(&contract_instance)?;
		info!(
			"*******EXECUTING EVM CONTRACT DEPLOYMENT******** CONTRACT ADDRESS: {:?}",
			hex::encode(contract_instance_address)
		);
		let event_data = "EVM Contract deployment succeeded";
		let event = Event::new(
			transaction_hash.clone(),
			event_data.as_bytes().to_vec(),
			block_number,
			EventType::EVM as i8,
			*contract_instance_address,
			None,
		);
		updated_state.create_event(&event)?;
		Ok(contract_instance_address.to_vec())
	}
}

#[async_trait]
impl<'a, 'g> ContractExecutor<'a, 'g> for ExecuteContract {
	fn execute_contract_deployment(
		&self,
		account_address: &Address,
		scheme: CreateScheme,
		_context: Context,
		cluster_address: &Address,
		access_type: AccessType,
		contract_code: &ContractCode,
		nonce: Nonce,
		gas_limit: Gas,
		deposit: Balance,
		transaction_hash: &TransactionHash,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> Result<(Address, Gas), Error> {
		let contract_instance_address = create_evm_address(scheme, nonce);
		let kind = RuntimeKind::Create(contract_instance_address);
		if updated_state.is_valid_account(&contract_instance_address.0)? {
			Err(anyhow!("EVM Deployment: Account collision, address: {:?}", contract_instance_address))?;
		}
		let account = Account::new_system(contract_instance_address.0);
		// creating account for execution address
		updated_state.create_account(&account)?;

		if deposit > 0 {
			match ExecuteToken::just_transfer_tokens(
				account_address,
				&contract_instance_address.0,
				deposit,
				updated_state,
			) {
				Err(e) => {
					Err(anyhow!("Can't transfer deposit from {} to {}, error: {:?}",
					hex::encode(account_address), hex::encode(&contract_instance_address.0), e))?
				},
				_ => ()
			}
		}

		let context = Context {
			address: contract_instance_address,
			caller: account_address.into(),
			apparent_value: deposit.into(),
		};

		let (result, gas) = self
			.execute_evm_runtime(
				account_address,
				cluster_address,
				access_type,
				contract_code,
				&vec![],
				nonce,
				&transaction_hash,
				gas_limit,
				false,
				block_number,
				block_hash,
				block_timestamp,
				kind,
				context,
				current_call_depth,
				updated_state,
				event_tx,
			);

		match result {
			Capture::Exit(reason) => {
				let (reason, _result) = reason;
				match reason {
					ExitReason::Succeed(_) => (),
					ExitReason::Error(e) => {
						Err(anyhow!("execute_contract_deployment: {:?}", e))?
					},
					ExitReason::Fatal(e) => {
						Err(anyhow!("execute_contract_deployment: {:?}", e))?
					}
					ExitReason::Revert(e) => {
						Err(anyhow!("execute_contract_deployment: {:?}", e))?
					},
				}
			},
			Capture::Trap(_resolve) => {
				Err(anyhow!("execute_contract_deployment: trap"))?
			},
		}
		Ok((contract_instance_address.into(), gas))
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
		_function: &ContractFunction,
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
		let contract_instance_address = contract_instance.instance_address;
		let kind = RuntimeKind::Call(H160(contract_instance_address));

		// This function is called from EVM call handler. There are 4 possible cases:
		// 1. CALL and CALLCODE - deposit can be greater then 0
		// 2. STATICCALL - deposit is 0 and is_read_only is true
		// 3. DELEGATECALL - deposit is 0
		if !is_read_only && deposit > 0 {
			match ExecuteToken::just_transfer_tokens(
				account_address,
				&contract_instance_address,
				deposit,
				updated_state,
			) {
				Err(e) => {
					return (
						Capture::Exit((
							ExitReason::Error(ExitError::Other(Cow::Owned(format!(
								"Can't transfer deposit from {} to {}, error: {:?}",
								hex::encode(&account_address), hex::encode(&contract_instance_address), e
							)))),
							Arc::new(vec![]),
						)),
						0,
					);
				},
				_ => ()
			}
		}

		self.execute_evm_runtime(
			account_address,
			cluster_address,
			contract.access,
			&contract.code,
			arguments,
			nonce,
			transaction_hash,
			gas_limit,
			is_read_only,
			block_number,
			block_hash,
			block_timestamp,
			kind,
			context,
			current_call_depth,
			updated_state,
			event_tx,
		)
	}

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
		let deposit = 0;
		let context = Context {
			// Execution address.
			address: H160(contract_instance.instance_address), // Does not affect pool id
			// Caller of the EVM.
			caller: H160::zero(), // Does not affect pool id
			// Apparent value of the EVM.
			apparent_value: U256::from(deposit),
		};
		self.execute_contract_function_call(
			&contract_instance.instance_address,
			context,
			gas_limit,
			deposit,
			cluster_address, //cluster address
			contract,
			contract_instance,
			function,
			arguments,
			nonce,
			block_hash,
			true,
			block_number,
			block_hash,
			block_timestamp,
			current_call_depth,
			updated_state,
			event_tx,
		)
	}
}

impl<'a, 'g> EVMContractCallTrait<'a, 'g> for ExecuteContract {
	fn execute_evm_contract_deployment(
		&self,
		account_address: &Address,
		scheme: CreateScheme,
		context: Context,
		cluster_address: &Address,
		access_type: AccessType,
		contract_code: &ContractCode,
		nonce: Nonce,
		gas_limit: Gas,
		deposit: Balance,
		transaction_hash: &TransactionHash,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		block_timestamp: BlockTimeStamp,
		current_call_depth: u32,
		updated_state: &mut UpdatedState,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> Result<(Address, Gas), Error> {
		self.execute_contract_deployment(
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

	fn execute_evm_contract_call(
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

	fn execute_evm_function_call_readonly(
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
		self.execute_contract_function_call_read_only(
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
