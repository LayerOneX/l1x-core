use anyhow::Error;
use borsh::BorshDeserialize;
use l1x_consensus_primitives::{
	Address, Balance, BlockHash, BlockNumber, BlockTimeStamp, ContractArgument, ContractFunction,
	ContractInstanceKey, ContractInstanceValue, EventData, Gas,
};
use l1x_consensus_traits::traits_l1xvm::{CallContractOutcome, VMContractTrait};
use l1x_consensus_util::generic::seconds_to_milliseconds;
use log::debug;
use num_traits::Zero;
use rbpf::{ebpf::HelperResult, runtime_config::RuntimeGas};
use vm_execution_fee::execution_fees::VmExecutionFees;

use crate::{
	error::{HostError, VMLogicError},
	util::MemSlice, EbpfObjectVersion,
};
use std::{
	cell::RefCell,
	collections::{hash_map::Entry, HashMap},
	rc::Rc,
	sync::Arc,
};

pub const CURRENT_RUNTIME_VERSION: u64 = 4;
pub const RUNTIME_VERSION_DEPOSIT_FEATURE: u64 = 5;

// Registers are a nice abstraction that allows developers to store data without moving it outside
// of VM.
type RegisterId = u64;
// An address in virtual memory.
type MemoryAddress = u64;
// Result type for VM logic
pub type Result<T, E = VMLogicError> = ::std::result::Result<T, E>;

pub struct BlockchainEnvironment {
	block_number: BlockNumber,
	block_hash: BlockHash,
	block_timestamp: BlockTimeStamp,
	deposit: Balance,
}

pub enum ContractCallVersion {
	V1,
	V2,
	V3,
}

#[derive(BorshDeserialize)]
struct ContractCallv1 {
	contract_instance_address: Address,
	method_name: String,
	args: Vec<u8>,
	read_only: bool,
	fee_limit: u128,
}

#[derive(BorshDeserialize)]
struct ContractCallv2 {
	contract_instance_address: Address,
	method_name: String,
	args: Vec<u8>,
	read_only: bool,
	gas_limit: Gas,
}

#[derive(BorshDeserialize)]
struct ContractCallv3 {
	contract_instance_address: Address,
	method_name: String,
	args: Vec<u8>,
	read_only: bool,
	gas_limit: Gas,
	deposit: Balance,
}

pub struct Memory {
	pub max_pages: u64,
	pub shared_memory_buffer: Vec<u8>,
}

impl Memory {
	fn read_bytes<const N: usize>(&self, offset: usize) -> Result<[u8; N]> {
		self.shared_memory_buffer
			.get(offset..offset + N)
			.ok_or(VMLogicError::from(HostError::MemoryAccessViolation))?
			.try_into()
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))
	}

	pub fn read_byte(&self, offset: usize) -> Result<[u8; 1]> {
		self.read_bytes(offset)
	}

	pub fn read_bytes2(&self, offset: usize) -> Result<[u8; 2]> {
		self.read_bytes(offset)
	}

	pub fn read_bytes4(&self, offset: usize) -> Result<[u8; 4]> {
		self.read_bytes(offset)
	}
	pub fn read_bytes8(&self, offset: usize) -> Result<[u8; 8]> {
		self.read_bytes(offset)
	}

	fn write_bytes<const N: usize>(&mut self, offset: usize, data: [u8; N]) -> Result<()> {
		if self.shared_memory_buffer.len() < offset + N {
			return Err(VMLogicError::from(HostError::MemoryAccessViolation))
		}
		self.shared_memory_buffer.splice(offset..offset + N, data);

		Ok(())
	}

	pub fn write_byte(&mut self, offset: usize, data: [u8; 1]) -> Result<()> {
		self.write_bytes(offset, data)
	}

	pub fn write_bytes2(&mut self, offset: usize, data: [u8; 2]) -> Result<()> {
		self.write_bytes(offset, data)
	}

	pub fn write_bytes4(&mut self, offset: usize, data: [u8; 4]) -> Result<()> {
		self.write_bytes(offset, data)
	}

	pub fn write_bytes8(&mut self, offset: usize, data: [u8; 8]) -> Result<()> {
		self.write_bytes(offset, data)
	}
}

pub struct BlockchainSyncApi<'a> {
	api: Arc<RefCell<&'a mut dyn VMContractTrait>>,
}

impl<'a> BlockchainSyncApi<'a> {
	pub fn new(api: Arc<RefCell<&'a mut dyn VMContractTrait>>) -> Self {
		Self { api }
	}

	fn contract_instance_owner_address_of(
		&self,
		contract_instance_address: Address,
	) -> Result<Address, Error> {
		let manager = self.api.borrow();
		manager.contract_instance_owner_address_of(contract_instance_address)
	}

	fn contract_code_address_of(
		&self,
		contract_instance_address: Address,
	) -> Result<Address, Error> {
		let manager = self.api.borrow();
		manager.contract_code_address_of(contract_instance_address)
	}

	fn contract_code_owner_address_of(
		&self,
		contract_instance_address: Address,
	) -> Result<Address, Error> {
		let manager = self.api.borrow();
		manager.contract_code_owner_address_of(contract_instance_address)
	}

	fn storage_read(
		&mut self,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		let mut manager = self.api.borrow_mut();
		manager.storage_read(key)
	}

	fn storage_remove(&mut self, key: &ContractInstanceKey) -> Result<(), Error> {
		let mut manager = self.api.borrow_mut();
		manager.storage_remove(key)
	}

	fn storage_write(
		&mut self,
		key: ContractInstanceKey,
		value: ContractInstanceValue,
	) -> Result<(), Error> {
		let mut manager = self.api.borrow_mut();
		manager.storage_write(key, value)
	}

	fn get_balance(&self, address: Address) -> Result<Balance, Error> {
		let manager = self.api.borrow_mut();
		manager.get_balance(address)
	}

	fn transfer_token(
		&mut self,
		from_address: Address,
		to_address: Address,
		amount: Balance,
	) -> Result<(), Error> {
		let mut manager = self.api.borrow_mut();
		manager.transfer_token(from_address, to_address, amount)
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
		let mut manager = self.api.borrow_mut();
		manager.call_contract(contract_instance_address, function, arguments, gas_limit, deposit, readonly)
	}

	fn emit_event(&mut self, event_data: EventData) -> Result<(), Error> {
		let mut manager = self.api.borrow_mut();
		manager.emit_event(event_data)
	}
}

pub struct VMLogic<'a> {
	pub registers: HashMap<RegisterId, Box<[u8]>>,
	pub api: BlockchainSyncApi<'a>,
	pub memory: Memory,
	pub stack_frames: HashMap<u64, Vec<Vec<u8>>>,
	pub caller_address: Address,
	pub contract_code_address: Address,
	pub contract_instance_address: Address,
	pub contract_owner_address: Address,
	pub input: ContractArgument,
	pub return_data: Vec<u8>,
	pub readonly: bool,
	pub runtime_gas: Rc<RefCell<RuntimeGas>>,
	pub execution_fees: VmExecutionFees,
	pub ebpf_object_version: EbpfObjectVersion,
	pub env: BlockchainEnvironment,
}

impl<'a> VMLogic<'a> {
	pub fn new(
		input: ContractArgument,
		caller_address: Address,
		contract_code_address: Address,
		contract_instance_address: Address,
		contract_owner_address: Address,
		memory: Memory,
		block_number: BlockNumber,
		block_hash: BlockHash,
		block_timestamp: BlockTimeStamp,
		readonly: bool,
		gas_limit: Gas,
		deposit: Balance,
		execution_fees: VmExecutionFees,
		ebpf_object_version: EbpfObjectVersion,
		api: Arc<RefCell<&'a mut dyn VMContractTrait>>,
	) -> Self {
		VMLogic {
			api: BlockchainSyncApi::new(api),
			input,
			memory,
			stack_frames: HashMap::new(),
			caller_address,
			contract_code_address,
			contract_instance_address,
			contract_owner_address,
			registers: HashMap::new(),
			return_data: Vec::new(),
			readonly,
			runtime_gas: Rc::new(RefCell::new(RuntimeGas::new(gas_limit))),
			execution_fees,
			ebpf_object_version,
			env: BlockchainEnvironment { block_number, block_hash, block_timestamp, deposit },
		}
	}

	pub fn runtime_gas_ref(&self) -> Rc<RefCell<RuntimeGas>> {
		self.runtime_gas.clone()
	}

	pub fn from_ptr(ptr: usize) -> Option<&'static mut Self> {
		// Refactored the function to be more secured.
		if ptr.is_zero() {
			return None
		}

		let ptr = ptr as *mut VMLogic;

		// This unsafe block is now only used to check the validity of the pointer.
		unsafe {
			if !ptr.is_null() {
				return Some(&mut *ptr)
			}
		}

		None
	}

	fn write_perm(&self) -> Result<(), VMLogicError> {
		if self.readonly {
			Err(VMLogicError::HostError(HostError::ReadonlyCall))
		} else {
			Ok(())
		}
	}

	fn try_burn_gas(&mut self, gas: Gas) -> Result<(), VMLogicError> {
		self.runtime_gas.borrow_mut().burn(gas);

		if self.runtime_gas.borrow().left() == 0 {
			return Err(VMLogicError::from(HostError::OutOfGas))
		} else {
			Ok(())
		}
	}

	fn mem_view(memory: &Memory, slice: MemSlice) -> Result<Vec<u8>, ()> {
		let shared_memory = &memory.shared_memory_buffer;
		if shared_memory.get(slice.addr as usize + slice.len as usize).is_none() {
			return Err(())
		}
		let mut res = Vec::<u8>::new();
		res.reserve_exact(slice.len as usize);
		for v in slice.addr..(slice.addr + slice.len) {
			res.push(shared_memory[v as usize]);
		}
		Ok(res)
	}

	fn mem_write(memory: &mut Memory, offset: MemoryAddress, buffer: &[u8]) -> Result<(), ()> {
		let shared_memory = &mut memory.shared_memory_buffer;
		if buffer.len() > shared_memory.len() - offset as usize {
			return Err(())
		}
		for (dst, src) in shared_memory[offset as usize..].iter_mut().zip(buffer.iter()) {
			*dst = *src
		}
		Ok(())
	}

	fn read_l1x_address(&self, address_ptr: MemoryAddress, address_len: u64) -> Result<Address> {
		let address =
			Self::mem_view(&self.memory, MemSlice { addr: address_ptr, len: address_len })
				.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
		let address: Address =
			address.try_into().map_err(|_| VMLogicError::from(HostError::InvalidAddress))?;

		Ok(address)
	}

	fn read_balance(&self, balance_ptr: MemoryAddress, balance_len: u64) -> Result<Balance> {
		let balance =
			Self::mem_view(&self.memory, MemSlice { addr: balance_ptr, len: balance_len })
				.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;

		let balance: [u8; 16] =
			balance.try_into().map_err(|_| VMLogicError::from(HostError::InvalidArgument))?;

		Ok(Balance::from_le_bytes(balance))
	}

	pub fn panic(&self, msg_ptr: MemoryAddress, msg_len: u64) -> Result<()> {
		if msg_len > 0 {
			let msg = Self::mem_view(&self.memory, MemSlice { addr: msg_ptr, len: msg_len })
				.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
			let panic_msg = String::from_utf8_lossy(&msg).to_string();
			Err(HostError::GuestPanic { panic_msg }.into())
		} else {
			Err(HostError::GuestPanic {
				panic_msg: "the contract panicked without the message".to_string(),
			}
			.into())
		}
	}

	pub fn read_register(
		&mut self,
		register_id: RegisterId,
		result_addr: MemoryAddress,
	) -> Result<()> {
		let res = if let Some(data) = self.registers.get(&register_id) {
			Self::mem_write(&mut self.memory, result_addr, &data[..])
				.map_err(|_| HostError::MemoryAccessViolation.into())
		} else {
			Err(HostError::InvalidRegisterId { register_id }.into())
		};
		res
	}

	pub fn register_len(&self, register_id: RegisterId) -> Result<u64> {
		if let Some(data) = self.registers.get(&register_id) {
			let len = data.len();
			Ok(len as u64)
		} else {
			Ok(u64::MAX)
		}
	}

	pub fn write_register(
		&mut self,
		register_id: RegisterId,
		data_addr: MemoryAddress,
		data_len: u64,
	) -> Result<()> {
		let data = Self::mem_view(&self.memory, MemSlice { addr: data_addr, len: data_len })
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
		match self.registers.entry(register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(data.into());
			},
			Entry::Vacant(entry) => {
				entry.insert(data.into());
			},
		};
		Ok(())
	}

	pub fn storage_read(
		&mut self,
		key_addr: MemoryAddress,
		key_len: u64,
		register_id: RegisterId,
	) -> Result<u64> {
		let key_data = Self::mem_view(&self.memory, MemSlice { addr: key_addr, len: key_len })
			// Should return Option<Vec<u8>>
			// Should receive &key_data
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
		let value = self
			.api
			.storage_read(&key_data)
			.map_err(|e| VMLogicError::from(HostError::StorageError { msg: e.to_string() }))?;
		if let Some(value) = value {
			self.registers.insert(register_id, value.into());
			Ok(1)
		} else {
			Ok(0)
		}
	}

	pub fn storage_write(
		&mut self,
		key_addr: MemoryAddress,
		key_len: u64,
		value_addr: MemoryAddress,
		value_len: u64,
		evicted_register_id: RegisterId,
	) -> Result<u64> {
		self.write_perm()?;
		let key_data = Self::mem_view(&self.memory, MemSlice { addr: key_addr, len: key_len })
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
		let value_data =
			Self::mem_view(&self.memory, MemSlice { addr: value_addr, len: value_len })
				.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
		let value_data_len = value_data.len();
		let evicted = self
			.api
			.storage_read(&key_data)
			.map_err(|e| VMLogicError::from(HostError::StorageError { msg: e.to_string() }))?;
		self.api
			.storage_write(key_data, value_data)
			.map_err(|e| VMLogicError::from(HostError::StorageError { msg: e.to_string() }))?;

		let gas_per_byte = self
			.execution_fees
			.storage_write_per_byte
			.checked_mul(value_data_len as _)
			.ok_or(VMLogicError::from(HostError::InvalidArgument))?;
		let total_gas = self
			.execution_fees
			.storage_write
			.checked_add(gas_per_byte)
			.ok_or(VMLogicError::from(HostError::InvalidArgument))?;
		self.try_burn_gas(total_gas)?;

		if let Some(evicted) = evicted {
			self.registers.insert(evicted_register_id, evicted.into());
			Ok(1)
		} else {
			Ok(0)
		}
	}

	pub fn storage_remove(
		&mut self,
		key_addr: MemoryAddress,
		key_len: u64,
		evicted_register_id: RegisterId,
	) -> Result<u64> {
		self.write_perm()?;
		let key_data = Self::mem_view(&self.memory, MemSlice { addr: key_addr, len: key_len })
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
		let evicted = self
			.api
			.storage_read(&key_data)
			.map_err(|e| VMLogicError::from(HostError::StorageError { msg: e.to_string() }))?;
		self.api
			.storage_remove(&key_data)
			.map_err(|e| VMLogicError::from(HostError::StorageError { msg: e.to_string() }))?;

		self.try_burn_gas(self.execution_fees.storage_remove)?;

		if let Some(evicted) = evicted {
			self.registers.insert(evicted_register_id, evicted.into());
			Ok(1)
		} else {
			Ok(0)
		}
	}

	pub fn storage_write_perm(&self) -> Result<u64> {
		match self.write_perm() {
			Ok(_) => Ok(1),
			Err(_) => Ok(0),
		}
	}

	pub fn input(&mut self, result_register_id: RegisterId) -> Result<()> {
		let data = self.input.as_slice();
		match self.registers.entry(result_register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(data.into());
			},
			Entry::Vacant(entry) => {
				entry.insert(data.into());
			},
		};
		Ok(())
	}

	pub fn output(&mut self, output_addr: u64, output_len: u64) -> Result<()> {
		let data = Self::mem_view(&self.memory, MemSlice { addr: output_addr, len: output_len })
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
		self.return_data = (*data).to_vec();
		Ok(())
	}

	pub fn msg(&mut self, addr: MemoryAddress, len: u64) -> Result<()> {
		let buf = Self::mem_view(&self.memory, MemSlice { addr, len })
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;
		let msg =
			String::from_utf8(buf).map_err(|_| VMLogicError::from(HostError::MalformedUtf8))?;
		println!("{}", msg);
		Ok(())
	}

	pub fn caller_address(&mut self, result_register_id: RegisterId) -> Result<()> {
		let data = self.caller_address;
		match self.registers.entry(result_register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(data.into());
			},
			Entry::Vacant(entry) => {
				entry.insert(data.into());
			},
		};
		Ok(())
	}

	pub fn contract_owner_address(&mut self, result_register_id: RegisterId) -> Result<()> {
		let data = self.contract_owner_address;
		match self.registers.entry(result_register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(data.into());
			},
			Entry::Vacant(entry) => {
				entry.insert(data.into());
			},
		};
		Ok(())
	}

	pub fn contract_owner_address_of(
		&mut self,
		address_ptr: MemoryAddress,
		len: u64,
		result_register_id: RegisterId,
	) -> Result<()> {
		let instance_address = self.read_l1x_address(address_ptr, len)?;
		let contract_owner_address = if instance_address == self.contract_instance_address {
			self.contract_owner_address
		} else {
			self.api
				.contract_instance_owner_address_of(instance_address)
				.map_err(|_| VMLogicError::HostError(HostError::InvalidAddress))?
		};
		let data = contract_owner_address;
		match self.registers.entry(result_register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(data.into());
			},
			Entry::Vacant(entry) => {
				entry.insert(data.into());
			},
		};
		Ok(())
	}

	pub fn contract_instance_address(&mut self, result_register_id: RegisterId) -> Result<()> {
		let data = self.contract_instance_address;
		match self.registers.entry(result_register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(data.into());
			},
			Entry::Vacant(entry) => {
				entry.insert(data.into());
			},
		};
		Ok(())
	}

	pub fn contract_code_address_of(
		&mut self,
		address_ptr: MemoryAddress,
		len: u64,
		result_register_id: RegisterId,
	) -> Result<()> {
		let instance_address = self.read_l1x_address(address_ptr, len)?;
		let code_address = if instance_address == self.contract_instance_address {
			self.contract_code_address
		} else {
			self.api
				.contract_code_address_of(instance_address)
				.map_err(|_| VMLogicError::HostError(HostError::InvalidAddress))?
		};
		let data = code_address;
		match self.registers.entry(result_register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(data.into());
			},
			Entry::Vacant(entry) => {
				entry.insert(data.into());
			},
		};
		Ok(())
	}

	pub fn contract_code_owner_address_of(
		&mut self,
		address_ptr: MemoryAddress,
		len: u64,
		result_register_id: RegisterId,
	) -> Result<()> {
		let code_address = self.read_l1x_address(address_ptr, len)?;
		let code_owner = self
			.api
			.contract_code_owner_address_of(code_address)
			.map_err(|_| VMLogicError::HostError(HostError::InvalidAddress))?;
		let data = code_owner;
		match self.registers.entry(result_register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(data.into());
			},
			Entry::Vacant(entry) => {
				entry.insert(data.into());
			},
		};
		Ok(())
	}

	pub fn gas_limit(&self) -> Result<Gas> {
		Ok(self.runtime_gas.borrow().gas_limit)
	}

	pub fn gas_left(&self) -> Result<Gas> {
		Ok(self.runtime_gas.borrow().left())
	}

	pub fn deposit(&mut self, result_addr: MemoryAddress, result_len: u64) -> Result<()> {
		let data = self.env.deposit.to_le_bytes();
		if result_len != data.len() as u64 {
			Err(HostError::MemoryAccessViolation.into())
		} else {
			Self::mem_write(&mut self.memory, result_addr, &data)
				.map_err(|_| HostError::MemoryAccessViolation.into())
		}
	}

	pub fn address_balance(
		&mut self,
		address_ptr: MemoryAddress,
		address_len: u64,
		result_register_id: RegisterId,
	) -> Result<()> {
		let address = self.read_l1x_address(address_ptr, address_len)?;
		let balance = self.api.get_balance(address).unwrap_or(0);

		match self.registers.entry(result_register_id) {
			Entry::Occupied(mut entry) => {
				entry.insert(Box::new(balance.to_le_bytes()));
			},
			Entry::Vacant(entry) => {
				entry.insert(Box::new(balance.to_le_bytes()));
			},
		};

		Ok(())
	}

	pub fn transfer_to(
		&mut self,
		to_ptr: MemoryAddress,
		to_len: u64,
		amount_ptr: MemoryAddress,
		amount_len: u64,
	) -> Result<u64> {
		let to_address = self.read_l1x_address(to_ptr, to_len)?;
		let amount = self.read_balance(amount_ptr, amount_len)?;
		let from_address = self.contract_instance_address.clone();

		self.try_burn_gas(self.execution_fees.token_transfer)?;

		match self.api.transfer_token(from_address, to_address, amount) {
			Ok(_) => Ok(1),
			Err(_) => Ok(0),
		}
	}

	pub fn transfer_from_caller(
		&mut self,
		amount_ptr: MemoryAddress,
		amount_len: u64,
	) -> Result<u64> {
		// This condtion is verified on the contract deployment step. 
		// The below check is required for the case when `trasnfer_from_caller` address is used directly
		if self.ebpf_object_version.expected_rutime_version >= RUNTIME_VERSION_DEPOSIT_FEATURE {
			Err(HostError::NotSupported(
				format!("tansfer_from_caller is not supported from runtime version '{}', object runtime version: {}",
					RUNTIME_VERSION_DEPOSIT_FEATURE, self.ebpf_object_version.expected_rutime_version)))?;
		}
		
		let amount = self.read_balance(amount_ptr, amount_len)?;
		let from_address = self.caller_address.clone();
		let to_address = self.contract_instance_address.clone();
		
		self.try_burn_gas(self.execution_fees.token_transfer)?;

		if self.env.deposit > 0 {
			if amount != self.env.deposit {
				Ok(0)
			} else {
				Ok(1)
			}
		} else {
			match self.api.transfer_token(from_address, to_address, amount) {
				Ok(_) => Ok(1),
				Err(_) => Ok(0),
			}
		}


	}

	pub fn block_hash(&mut self, result_addr: MemoryAddress, result_len: u64) -> Result<()> {
		if result_len != self.env.block_hash.len() as u64 {
			Err(HostError::MemoryAccessViolation.into())
		} else {
			Self::mem_write(&mut self.memory, result_addr, &self.env.block_hash)
				.map_err(|_| HostError::MemoryAccessViolation.into())
		}
	}

	pub fn block_number(&mut self, result_addr: MemoryAddress, result_len: u64) -> Result<()> {
		let data = self.env.block_number.to_le_bytes();
		if result_len != data.len() as u64 {
			Err(HostError::MemoryAccessViolation.into())
		} else {
			Self::mem_write(&mut self.memory, result_addr, &data)
				.map_err(|_| HostError::MemoryAccessViolation.into())
		}
	}

	pub fn block_timestamp(&mut self, result_addr: MemoryAddress, result_len: u64) -> Result<()> {
		let block_timestamp_ms = seconds_to_milliseconds(u128::from(self.env.block_timestamp));
		let data = block_timestamp_ms.to_le_bytes();
		if result_len != data.len() as u64 {
			Err(HostError::MemoryAccessViolation.into())
		} else {
			Self::mem_write(&mut self.memory, result_addr, &data)
				.map_err(|_| HostError::MemoryAccessViolation.into())
		}
	}

	pub fn call_contract(
		&mut self,
		version: ContractCallVersion,
		call_addr: MemoryAddress,
		call_len: u64,
		result_register_id: RegisterId,
	) -> Result<u64> {
		let call_data = Self::mem_view(&self.memory, MemSlice { addr: call_addr, len: call_len })
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;

		let call = match version {
			ContractCallVersion::V1 => {
				let call_v1 = ContractCallv1::try_from_slice(&call_data)
					.map_err(|_| VMLogicError::from(HostError::InvalidArgument))?;
				ContractCallv3 {
					contract_instance_address: call_v1.contract_instance_address,
					method_name: call_v1.method_name,
					args: call_v1.args,
					read_only: call_v1.read_only,
					gas_limit: Gas::try_from(call_v1.fee_limit)
						.map_err(|_| VMLogicError::from(HostError::InvalidArgument))?,
					deposit: 0,
				}
			},
			ContractCallVersion::V2 => {
				let call_v2 = ContractCallv2::try_from_slice(&call_data)
				.map_err(|_| VMLogicError::from(HostError::InvalidArgument))?;
				ContractCallv3 {
					contract_instance_address: call_v2.contract_instance_address,
					method_name: call_v2.method_name,
					args: call_v2.args,
					read_only: call_v2.read_only,
					gas_limit: call_v2.gas_limit,
					deposit: 0,
				}
			},
			// V3 will be supported from RUNTIME_VERSION_DEPOSIT_FEATURE
			ContractCallVersion::V3 => ContractCallv3::try_from_slice(&call_data)
				.map_err(|_| VMLogicError::from(HostError::InvalidArgument))?,
		};

		if self.readonly && !call.read_only {
			return Err(VMLogicError::from(HostError::ReadonlyCall))
		}

		self.try_burn_gas(self.execution_fees.cross_contract_call)?;

		let gas_left = self.runtime_gas.borrow().left();
		if gas_left < call.gas_limit {
			return Err(VMLogicError::from(HostError::ContractCallGasLimit { gas_left, gas_limit: call.gas_limit }))
		}

		match self.api.call_contract(
			call.contract_instance_address,
			call.method_name.clone().into_bytes(),
			call.args.clone(),
			call.gas_limit,
			call.deposit,
			call.read_only,
		) {
			Ok(res) => {
				self.runtime_gas.borrow_mut().burn(res.burnt_gas);
				self.registers.insert(result_register_id, res.result.into_boxed_slice());
				return Ok(1)
			},
			Err(e) => {
				self.registers
					.insert(result_register_id, e.to_string().into_boxed_str().into_boxed_bytes());
				return Ok(0)
			},
		}
	}

	pub fn emit_event(&mut self, data_addr: MemoryAddress, data_len: u64) -> Result<u64> {
		self.write_perm()?;

		let event_data = Self::mem_view(&self.memory, MemSlice { addr: data_addr, len: data_len })
			.map_err(|_| VMLogicError::from(HostError::MemoryAccessViolation))?;

		let gas_per_byte = self
			.execution_fees
			.emit_event_per_bytes
			.checked_mul(event_data.len() as _)
			.ok_or(VMLogicError::from(HostError::InvalidArgument))?;
		let total_gas = self
			.execution_fees
			.emit_event
			.checked_add(gas_per_byte)
			.ok_or(VMLogicError::from(HostError::InvalidArgument))?;
		self.try_burn_gas(total_gas)?;

		match self.api.emit_event(event_data) {
			Ok(_) => Ok(1),
			Err(_) => Ok(0),
		}
	}
}

pub fn env_msg(ctx: usize, r1: u64, r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	debug!("Called env_msg");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.msg(r1, r2);
	Ok(0)
}

pub fn env_panic(ctx: usize, _r1: u64, _r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	debug!("Called env_panic");
	// This version of "panic" API doesn't receive arguments
	let msg_ptr = 0;
	let msg_len = 0;
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.panic(msg_ptr, msg_len).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_panic_msg(ctx: usize, r1: u64, r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	debug!("Called env_panic_msg");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.panic(r1, r2).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_input(ctx: usize, r1: u64, _r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	debug!("Called env_input");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.input(r1);
	Ok(0)
}

pub fn env_output(ctx: usize, r1: u64, r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	debug!("Called env_output");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.output(r1, r2).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_register_len(
	ctx: usize,
	r1: u64,
	_r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_register_len {r1}");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let regi_len = vm_logic.register_len(r1).map_err(|e_val| e_val.to_string())?;
	Ok(regi_len)
}
pub fn env_read_register(
	ctx: usize,
	r1: u64,
	r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_read_register");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.read_register(r1, r2).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}
pub fn env_write_register(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_write_register");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.write_register(r1, r2, r3).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}
pub fn env_storage_write(ctx: usize, r1: u64, r2: u64, r3: u64, r4: u64, r5: u64) -> HelperResult {
	debug!("Called env_storage_write");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let w_rval = vm_logic.storage_write(r1, r2, r3, r4, r5).map_err(|e_val| e_val.to_string())?;
	Ok(w_rval)
}
pub fn env_storage_remove(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_storage_remove");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let rm_rval = vm_logic.storage_remove(r1, r2, r3).map_err(|e_val| e_val.to_string())?;
	Ok(rm_rval)
}
pub fn env_storage_read(ctx: usize, r1: u64, r2: u64, r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	debug!("Called env_storage_read");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let sr_rval = vm_logic.storage_read(r1, r2, r3).map_err(|e_val| e_val.to_string())?;
	Ok(sr_rval)
}

pub fn env_storage_write_perm(
	ctx: usize,
	_r1: u64,
	_r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_storage_write_perm");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let rm_rval = vm_logic.storage_write_perm().map_err(|e_val| e_val.to_string())?;
	Ok(rm_rval)
}

pub fn env_current_runtime_version(
	_ctx: usize,
	_r1: u64,
	_r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_current_runtime_version");
	Ok(CURRENT_RUNTIME_VERSION)
}

pub fn env_caller_address(
	ctx: usize,
	r1: u64,
	_r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_caller_address");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.caller_address(r1).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_contract_owner_address(
	ctx: usize,
	r1: u64,
	_r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_contract_owner_address");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.contract_owner_address(r1).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_contract_owner_address_of(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_contract_owner_address_of");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic
		.contract_owner_address_of(r1, r2, r3)
		.map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_contract_instance_address(
	ctx: usize,
	r1: u64,
	_r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_contract_instance_address");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.contract_instance_address(r1).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_contract_code_address_of(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_contract_code_address_of");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic
		.contract_code_address_of(r1, r2, r3)
		.map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_contract_code_owner_address_of(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_contract_code_owner_address_of");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic
		.contract_code_owner_address_of(r1, r2, r3)
		.map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_gas_limit(ctx: usize, _r1: u64, _r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.gas_limit().map_err(|e_val| e_val.to_string())
}

pub fn env_gas_left(ctx: usize, _r1: u64, _r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.gas_left().map_err(|e_val| e_val.to_string())
}

pub fn env_deposit(ctx: usize, r1: u64, r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.deposit(r1, r2).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_address_balance(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.address_balance(r1, r2, r3).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_transfer_to(ctx: usize, r1: u64, r2: u64, r3: u64, r4: u64, _r5: u64) -> HelperResult {
	debug!("Called env_transfer_to");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let ret = vm_logic.transfer_to(r1, r2, r3, r4).map_err(|e_val| e_val.to_string())?;
	Ok(ret)
}

pub fn env_transfer_from_caller(
	ctx: usize,
	r1: u64,
	r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_transfer_from_caller");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let ret = vm_logic.transfer_from_caller(r1, r2).map_err(|e_val| e_val.to_string())?;
	Ok(ret)
}

pub fn env_block_hash(ctx: usize, r1: u64, r2: u64, _r3: u64, _r4: u64, _r5: u64) -> HelperResult {
	debug!("Called env_block_hash");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.block_hash(r1, r2).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_block_number(
	ctx: usize,
	r1: u64,
	r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_block_number");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.block_number(r1, r2).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_block_timestamp(
	ctx: usize,
	r1: u64,
	r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_block_timestamp");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let _ = vm_logic.block_timestamp(r1, r2).map_err(|e_val| e_val.to_string())?;
	Ok(0)
}

pub fn env_call_contract(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_call_contract");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let call_rval = vm_logic
		.call_contract(ContractCallVersion::V1, r1, r2, r3)
		.map_err(|e_val| e_val.to_string())?;
	Ok(call_rval)
}

pub fn env_call_contract2(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called env_call_contract2");
	// Ensure ctx is a valid pointer before dereferencing.
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	let call_rval = vm_logic
		.call_contract(ContractCallVersion::V2, r1, r2, r3)
		.map_err(|e_val| e_val.to_string())?;
	Ok(call_rval)
}

pub fn emit_event_experimental(
	ctx: usize,
	r1: u64,
	r2: u64,
	_r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	debug!("Called emit_event_experimental");
	let vm_logic = VMLogic::from_ptr(ctx).ok_or_else(|| HostError::InValidCtx(ctx).to_string())?;
	vm_logic.emit_event(r1, r2).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
	use crate::{logic::Memory, MAX_SHARED_MEMORY_PAGES};

	#[test]
	pub fn shared_memory_read_write_test() {
		let shared_memory_buffer = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
		let mut memory = Memory { shared_memory_buffer, max_pages: MAX_SHARED_MEMORY_PAGES };

		assert!(memory.write_byte(1, [1]).is_ok());
		assert_eq!(memory.read_byte(1), Ok([1]));

		assert!(memory.write_bytes2(3, [2, 3]).is_ok());
		assert_eq!(memory.read_bytes2(3), Ok([2, 3]));

		assert!(memory.write_bytes4(4, [4, 5, 6, 7]).is_ok());
		assert_eq!(memory.read_bytes4(4), Ok([4, 5, 6, 7]));

		assert!(memory.write_bytes8(8, [8, 9, 10, 11, 12, 13, 14, 15]).is_ok());
		assert_eq!(memory.read_bytes8(8), Ok([8, 9, 10, 11, 12, 13, 14, 15]));
	}

	#[test]
	pub fn shared_memory_read_fail_test() {
		let shared_memory_buffer = vec![1, 2];
		let memory = Memory { shared_memory_buffer, max_pages: MAX_SHARED_MEMORY_PAGES };

		assert!(memory.read_byte(1).is_ok());
		assert!(memory.read_byte(2).is_err());
		assert!(memory.read_bytes2(0).is_ok());
		assert!(memory.read_bytes2(1).is_err());
		assert!(memory.read_bytes4(0).is_err());
		assert!(memory.read_bytes8(0).is_err());
	}

	#[test]
	pub fn shared_memory_write_fail_test() {
		let shared_memory_buffer = vec![1, 2];
		let mut memory = Memory { shared_memory_buffer, max_pages: MAX_SHARED_MEMORY_PAGES };

		assert!(memory.write_byte(1, [1]).is_ok());
		assert!(memory.write_byte(2, [2]).is_err());
		assert!(memory.write_bytes2(0, [2, 3]).is_ok());
		assert!(memory.write_bytes2(1, [2, 3]).is_err());
		assert!(memory.write_bytes4(0, [4, 5, 6, 7]).is_err());
		assert!(memory.write_bytes8(0, [8, 9, 10, 11, 12, 13, 14, 15]).is_err());
	}
}
