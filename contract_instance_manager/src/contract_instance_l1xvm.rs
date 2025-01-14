use crate::contract_instance_manager::ContractInstanceManager;
use anyhow::{anyhow, Error};
use evm_runtime::{Capture, Context, ExitReason, ExitSucceed};
use primitive_types::{H160, U256};
use primitives::*;
use std::sync::Arc;
use system::{
	contract::ContractType,
	event::{Event, EventType},
};
use traits::traits_l1xvm::{
	CallContractOutcome, VMContractTrait,
};
impl<'a, 'g> VMContractTrait for ContractInstanceManager<'a, 'g> {
	fn contract_instance_owner_address_of(
		&self,
		contract_instance_address: Address,
	) -> Result<Address, Error> {
		let contract_instance = self.updated_state.get_contract_instance(&contract_instance_address)?;

		Ok(contract_instance.owner_address)
	}

	fn contract_code_owner_address_of(
		&self,
		contract_code_address: Address,
	) -> Result<Address, Error> {
		let (_access, code_owner) = self.updated_state.get_contract_owner(&contract_code_address)?;

		Ok(code_owner)
	}

	fn contract_code_address_of(
		&self,
		contract_instance_address: Address,
	) -> Result<Address, Error> {
		let contract_instance = self.updated_state.get_contract_instance(&contract_instance_address)?;

		Ok(contract_instance.contract_address)
	}

	fn storage_read(
		&mut self,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		let key = key.clone();
		let instance_address = self.contract_instance.clone().unwrap().instance_address;

		self.updated_state.get_state_key_value(&instance_address, &key)
	}

	fn storage_remove(&mut self, key: &ContractInstanceKey) -> Result<(), Error> {
		let instance_address = self.contract_instance.clone().unwrap().instance_address;

		self.updated_state.delete_state_key_value(&instance_address, &key)
	}

	fn storage_write(
		&mut self,
		key: ContractInstanceKey,
		value: ContractInstanceValue,
	) -> Result<(), Error> {
		let instance_address = self.contract_instance.clone().unwrap().instance_address;

		self.updated_state.store_state_key_value(&instance_address, &key, &value)
	}

	fn transfer_token(
		&mut self,
		from_address: Address,
		to_address: Address,
		amount: Balance,
	) -> Result<(), Error> {
		self.updated_state.transfer(&from_address, &to_address, amount)
	}

	fn get_balance(&self, address: Address) -> Result<Balance, Error> {
		self.updated_state.get_balance(&address)
	}

	fn call_contract(
		&mut self,
		contract_instance_address: Address,
		function: ContractFunction,
		arguments: ContractArgument,
		gas_limit: Gas,
		deposit: Balance,
		readonly: bool,
	) -> Result<CallContractOutcome, Error> {
		let current_contract_instance_address =
			self.contract_instance.as_ref().unwrap().instance_address;

		let contract_instance = self.updated_state.get_contract_instance(&contract_instance_address)?;
		let contract = self.updated_state.get_contract(&contract_instance.contract_address)?;

		let mut cloned_updated_state = self.updated_state.clone();
		let current_call_depth = self.current_call_depth + 1;
		let (result, gas) = match contract.r#type.try_into()? {
			ContractType::L1XVM => {
				self.l1xvm.execute_contract_function_call(
					&current_contract_instance_address,
					&self.cluster_address, //cluster address
					&contract,
					&contract_instance,
					&function,
					&arguments,
					gas_limit,
					deposit,
					self.nonce,
					&self.transaction_hash,
					readonly,
					self.block_number,
					&self.block_hash,
					self.block_timestamp,
					current_call_depth,
					&mut cloned_updated_state,
					self.event_tx.clone(),
				)
			},
			ContractType::EVM => {
				let context = Context {
					address: H160::from(contract_instance_address.clone()),
					caller: H160::from(current_contract_instance_address),
					apparent_value: U256::from(deposit),
				};

				self.evm.execute_contract_function_call(
					&contract_instance_address,
					context,
					gas_limit,
					deposit,
					&self.cluster_address, //cluster address
					&contract,
					&contract_instance,
					&function,
					&arguments,
					self.nonce,
					&self.transaction_hash,
					readonly,
					self.block_number,
					&self.block_hash,
					self.block_timestamp,
					current_call_depth,
					&mut cloned_updated_state,
					self.event_tx.clone(),
				)
			},
			ContractType::XTALK =>
				(Capture::Exit((ExitReason::Succeed(ExitSucceed::Returned), Arc::new(vec![]))), 0),
		};
		match result {
			Capture::Exit(reason) => {
				let (reason, result) = reason;
				match reason {
					ExitReason::Succeed(_) => {
						self.updated_state.merge(&cloned_updated_state);
						Ok(CallContractOutcome { result: (*result).clone(), burnt_gas: gas })
					},
					ExitReason::Error(e) => Err(anyhow!("Error: {:?}", e)),
					ExitReason::Fatal(e) => Err(anyhow!("Fatal: {:?}", e)),
					ExitReason::Revert(e) => Err(anyhow!("Revert: {:?}", e)),
				}
			},
			Capture::Trap(_resolve) => Err(anyhow!("Trap")),
		}
	}

	fn emit_event(&mut self, event_data: EventData) -> Result<(), Error> {
		let transaction_hash = self.transaction_hash;
		let block_number = self.block_number;
		let contract_address = self.contract_instance
			.as_ref()
			.ok_or_else(|| anyhow!("Emit event | Contract instance not found"))?
			.instance_address;

		self.updated_state.create_event(&Event::new(
			transaction_hash,
			event_data,
			block_number,
			EventType::L1XVM as i8,
			contract_address,
			None,
		))
	}

	fn execute_remote_contract(
		&mut self,
		_cluster_address: Address,
		_contract_instance_address: Address,
		_function: ContractFunction,
		_arguments: Vec<ContractArgument>,
	) -> Result<TransactionHash, Error> {
		Ok([0u8; 32])
	}
}
