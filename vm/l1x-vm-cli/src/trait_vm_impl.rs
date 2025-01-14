use std::{
	collections::HashMap,
	os::unix::prelude::OsStrExt,
	path::PathBuf,
	rc::Rc,
	sync::{Arc, Mutex},
};

use anyhow::{anyhow, Error};
use l1x_consensus_primitives::*;
use l1x_consensus_traits::traits_l1xvm::{
	CallContractOutcome, VMContractTrait,
};
use l1x_rbpf::runtime_config::RuntimeGas;
use l1x_vm_execution_fee::execution_fees::VmExecutionFees;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::cell::RefCell;

use crate::helpers;

#[derive(Clone)]
pub struct BlockchainEnv {
	pub block_hash: BlockHash,
	pub block_number: BlockNumber,
	pub block_timestamp: BlockTimeStamp,
	pub runtime_gas: Rc<RefCell<RuntimeGas>>,
}

enum DbCachedValue {
	Cached(Option<ContractInstanceValue>),
	Updated(Option<ContractInstanceValue>),
}

pub struct CachedDB {
	db: DBWithThreadMode<MultiThreaded>,
	cache: HashMap<ContractInstanceKey, DbCachedValue>,
}

impl CachedDB {
	pub fn new(db_path: &str) -> Self {
		let mut opts = rocksdb::Options::default();
		opts.create_if_missing(true);
		// let db = rocksdb::DB::open(&opts, DB_PATH).unwrap();
		let db = rocksdb::DBWithThreadMode::<MultiThreaded>::open(&opts, db_path).unwrap();
		Self { db, cache: HashMap::new() }
	}

	pub fn get(
		&mut self,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		if let Some(value) = self.cache.get(key) {
			match value {
				DbCachedValue::Cached(v) => Ok(v.clone()),
				DbCachedValue::Updated(v) => Ok(v.clone()),
			}
		} else {
			let v = self.db.get(&key).map_err(|e| anyhow!(e.into_string()))?;
			self.cache.insert(key.clone(), DbCachedValue::Cached(v.clone()));
			Ok(v)
		}
	}

	pub fn delete(&mut self, key: ContractInstanceKey) -> Result<(), Error> {
		self.cache.insert(key.clone(), DbCachedValue::Updated(None));
		Ok(())
		// self.db.delete(key).map_err(|e| anyhow!(e.into_string()))
	}

	pub fn put(
		&mut self,
		key: ContractInstanceKey,
		value: ContractInstanceValue,
	) -> Result<(), Error> {
		self.cache.insert(key.clone(), DbCachedValue::Updated(Some(value)));
		Ok(())
	}

	pub fn commit(&mut self) {
		for (k, v) in &self.cache {
			match v {
				DbCachedValue::Updated(v) =>
					if let Some(v) = v {
						let _ = self.db.put(k, v.clone());
					} else {
						let _ = self.db.delete(k);
					},
				_ => (),
			}
		}
	}
}

pub struct VmApi {
	cached_db: Arc<Mutex<CachedDB>>,
	contract_address: Address,
	contract_path: PathBuf,
	env: BlockchainEnv,
}

impl<'a> VmApi {
	pub fn new(
		contract_path: &PathBuf,
		cached_db: Arc<Mutex<CachedDB>>,
		env: BlockchainEnv,
	) -> Self {
		let mut contract_address: Address = [0; 20];
		contract_path
			.file_stem()
			.unwrap()
			.as_bytes()
			.iter()
			.zip(contract_address.iter_mut())
			.for_each(|(src, dst)| *dst = *src);
		Self { contract_address, cached_db, contract_path: contract_path.clone(), env }
	}

	pub fn commit_changes(&mut self) {
		let mut db = self.cached_db.lock().expect("Can't lock cached_db");
		db.commit();
	}

	fn prefix_key(&self, key: &ContractInstanceKey) -> ContractInstanceKey {
		let mut v = Vec::from(self.contract_address);
		v.push(0xff);
		v.extend(key.iter());
		v
	}
}

impl<'a> VMContractTrait for VmApi {
	fn contract_instance_owner_address_of(
		&self,
		_contract_instance_address: Address,
	) -> Result<Address, Error> {
		unimplemented!()
	}

	fn contract_code_owner_address_of(
		&self,
		_contract_code_address: Address,
	) -> Result<Address, Error> {
		unimplemented!()
	}

	fn contract_code_address_of(
		&self,
		_contract_instance_address: Address,
	) -> Result<Address, Error> {
		unimplemented!()
	}

	fn storage_read(
		&mut self,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		let preixed_key = self.prefix_key(key);
		let mut db = self.cached_db.lock().map_err(|e| anyhow!(e.to_string()))?;
		db.get(&preixed_key)
	}

	fn storage_remove(&mut self, key: &ContractInstanceKey) -> Result<(), Error> {
		let preixed_key = self.prefix_key(key);
		let mut db = self.cached_db.lock().map_err(|e| anyhow!(e.to_string()))?;
		db.delete(preixed_key)
	}

	fn storage_write(
		&mut self,
		key: ContractInstanceKey,
		value: ContractInstanceValue,
	) -> Result<(), Error> {
		let preixed_key = self.prefix_key(&key);
		let mut db = self.cached_db.lock().map_err(|e| anyhow!(e.to_string()))?;
		db.put(preixed_key, value)
	}

	fn transfer_token(
		&mut self,
		from_address: Address,
		to_address: Address,
		amount: Balance,
	) -> Result<(), Error> {
		println!(
			"Emulated token transfer: From {:?} to {:?} of amount {}",
			from_address, to_address, amount
		);
		Ok(())
	}

	fn get_balance(&self, _address: Address) -> Result<Balance, Error> {
		Ok(1_000)
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
		let mut contract_path = self.contract_path.clone();
		let contract_instance_address_vec = Vec::from(contract_instance_address);
		let file_name = String::from_utf8_lossy(&contract_instance_address_vec);
		contract_path.pop();
		contract_path.push(file_name.to_string().trim_matches(char::from(0)));
		contract_path.set_extension("o");

		let mut vm_api = VmApi::new(&contract_path, self.cached_db.clone(), self.env.clone());
		let elf_bytes = std::fs::read(contract_path.clone());
		if let Err(err) = elf_bytes {
			eprintln!("ERROR: {}, filename={:?}", err, contract_path);
			return Err(err.into())
		}

		let owner_address = {
			let mut bytes = contract_instance_address_vec.clone();
			// Generate an unique address for the owner
			bytes[0] += 3;
			bytes
		};

		let elf_bytes = elf_bytes.unwrap();
		let owner_address = owner_address.clone().try_into().expect(&format!(
			"Can't create contract_owner address from Vec length {}",
			owner_address.len()
		));

		// Derive the code address for debug purposes. This information is fethed from
		// contract_instance in Consensus code.
		let contract_code_address = helpers::derive_code_address_from(contract_instance_address);

		let res = l1x_ebpf_runtime::run(
			&elf_bytes,
			function,
			arguments,
			self.contract_address.clone(),
			contract_code_address,
			contract_instance_address,
			owner_address,
			self.env.block_number,
			self.env.block_hash,
			self.env.block_timestamp,
			readonly,
			gas_limit,
			deposit,
			VmExecutionFees::create_basic_config(),
			Arc::new(RefCell::new(&mut vm_api)),
		)?;

		Ok(CallContractOutcome { result: res.result.into_bytes(), burnt_gas: res.burnt_gas })
	}

	// Inter cluster is not yet implemented
	fn execute_remote_contract(
		&mut self,
		_cluster_address: Address,
		_contract_instance_address: Address,
		_function: ContractFunction,
		_arguments: Vec<ContractArgument>,
	) -> Result<TransactionHash, Error> {
		unimplemented!()
	}

	fn emit_event(&mut self, _event_data: EventData) -> Result<(), Error> {
		println!("Event: {:?}", _event_data);
		Ok(())
	}
}
