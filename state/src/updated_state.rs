use std::{cell::RefCell, collections::HashMap};
use log::info;
use anyhow::{anyhow, Error};
use db::db::DbTxConn;
use primitives::{
	AccessType, Address, Balance, BlockNumber, ContractCode, ContractInstanceKey,
	ContractInstanceValue, ContractType, Nonce,
};
use system::{
	account::Account, contract::Contract, contract_instance::ContractInstance, event::Event, staking_account::StakingAccount, staking_pool::StakingPool, transaction::TransactionMetadata
};

use crate::updated_state_db::DBRequest;

pub trait Updatable<T> {
	fn update(&mut self, v: &T);
}

#[derive(Debug, Clone)]
enum CachedStatus<T>
where
	T: Updatable<T> + Clone,
{
	Updated(Option<T>),
	Cached(T),
}

impl<T> CachedStatus<T>
where
	T: Updatable<T> + Clone,
{
	pub fn new_cached(v: T) -> Self {
		Self::Cached(v)
	}

	pub fn new_updated(v: Option<T>) -> Self {
		Self::Updated(v)
	}

	pub fn update(&mut self, v: Option<T>) {
		match self {
			CachedStatus::Cached(cached) => match v {
				Some(v) => {
					let mut new = Self::new_updated(Some(v));
					new.update(Some(cached.clone()));
					*self = new;
				},
				None => *self = Self::new_updated(None),
			},
			CachedStatus::Updated(Some(updated)) => match v {
				Some(v) => updated.update(&v),
				None => *self = Self::new_updated(None),
			},
			CachedStatus::Updated(None) => *self = Self::new_updated(v),
		}
	}

	pub fn update_cached(&mut self, v: &T) {
		match self {
			CachedStatus::Cached(_self) | CachedStatus::Updated(Some(_self)) => {
				_self.update(v);
			},
			CachedStatus::Updated(None) => {
				panic!("Update cache for Removed")
			},
		}
	}

	pub fn is_removed(&self) -> bool {
		matches!(self, CachedStatus::Updated(None))
	}

	pub fn get_inner(&self) -> Option<&T> {
		match self {
			CachedStatus::Cached(v) => Some(v),
			CachedStatus::Updated(v) => v.as_ref(),
		}
	}
}

#[derive(Debug, Clone)]
struct UpdatedAccount {
	pub account: Account,
}

impl Updatable<UpdatedAccount> for UpdatedAccount {
	fn update(&mut self, v: &UpdatedAccount) {
		self.account = v.account.clone()
	}
}

impl From<&Account> for UpdatedAccount {
	fn from(account: &Account) -> Self {
		Self { account: account.clone() }
	}
}

impl From<Account> for UpdatedAccount {
	fn from(account: Account) -> Self {
		Self::from(&account)
	}
}

#[derive(Debug, Clone, Default)]
struct UpdatedContract {
	pub address: Address,
	pub access: AccessType,
	pub r#type: ContractType,
	pub code: ContractCode,
	pub owner_address: Address,
}

impl Updatable<UpdatedContract> for UpdatedContract {
	fn update(&mut self, v: &UpdatedContract) {
		*self = v.clone()
	}
}

impl From<Contract> for UpdatedContract {
	fn from(contract: Contract) -> Self {
		UpdatedContract::from(&contract)
	}
}

impl From<&Contract> for UpdatedContract {
	fn from(contract: &Contract) -> Self {
		Self {
			address: contract.address,
			access: contract.access,
			r#type: contract.r#type,
			code: contract.code.clone(),
			owner_address: contract.owner_address,
		}
	}
}

impl Into<Contract> for &UpdatedContract {
	fn into(self) -> Contract {
		Contract {
			address: self.address,
			access: self.access,
			r#type: self.r#type,
			code: self.code.clone(),
			owner_address: self.owner_address,
		}
	}
}

impl Into<Contract> for UpdatedContract {
	fn into(self) -> Contract {
		(&self).into()
	}
}

#[derive(Debug, Clone, Default)]
struct UpdatedContractInstance {
	pub instance_address: Address,
	pub contract_address: Address,
	pub owner_address: Address,
}

impl Updatable<UpdatedContractInstance> for UpdatedContractInstance {
	fn update(&mut self, v: &UpdatedContractInstance) {
		*self = v.clone()
	}
}

impl From<&ContractInstance> for UpdatedContractInstance {
	fn from(v: &ContractInstance) -> Self {
		Self {
			contract_address: v.contract_address,
			instance_address: v.instance_address,
			owner_address: v.owner_address,
		}
	}
}

impl From<ContractInstance> for UpdatedContractInstance {
	fn from(v: ContractInstance) -> Self {
		Self::from(&v)
	}
}

impl Into<ContractInstance> for &UpdatedContractInstance {
	fn into(self) -> ContractInstance {
		ContractInstance {
			contract_address: self.contract_address,
			instance_address: self.instance_address,
			owner_address: self.owner_address,
		}
	}
}

impl Into<ContractInstance> for UpdatedContractInstance {
	fn into(self) -> ContractInstance {
		(&self).into()
	}
}

#[derive(Debug, Clone)]
struct UpdatedContractInstanceValue {
	pub value: Option<ContractInstanceValue>,
}

impl Updatable<UpdatedContractInstanceValue> for UpdatedContractInstanceValue {
	fn update(&mut self, v: &UpdatedContractInstanceValue) {
		self.value = v.value.clone();
	}
}

#[derive(Debug, Clone)]
pub struct UpdatedEvent {
	event: Event,
}

impl Updatable<UpdatedEvent> for UpdatedEvent {
	fn update(&mut self, v: &UpdatedEvent) {
		self.event = v.event.clone()
	}
}

impl From<&Event> for UpdatedEvent {
	fn from(value: &Event) -> Self {
		Self { event: value.clone() }
	}
}

impl Into<Event> for &UpdatedEvent {
	fn into(self) -> Event {
		self.event.clone()
	}
}

#[derive(Debug, Clone)]
pub struct UpdatedStakingPool {
	pool: StakingPool,
}

impl Updatable<UpdatedStakingPool> for UpdatedStakingPool {
	fn update(&mut self, v: &UpdatedStakingPool) {
		self.pool = v.pool.clone()
	}
}

impl From<&StakingPool> for UpdatedStakingPool {
	fn from(value: &StakingPool) -> Self {
		Self { pool: value.clone() }
	}
}

impl From<StakingPool> for UpdatedStakingPool {
	fn from(value: StakingPool) -> Self {
		Self::from(&value)
	}
}

impl Into<StakingPool> for &UpdatedStakingPool {
	fn into(self) -> StakingPool {
		self.pool.clone()
	}
}

#[derive(Debug, Clone)]
pub struct UpdatedStakingAccount {
	account: StakingAccount,
}

impl Updatable<UpdatedStakingAccount> for UpdatedStakingAccount {
	fn update(&mut self, v: &UpdatedStakingAccount) {
		self.account = v.account.clone()
	}
}

impl From<&StakingAccount> for UpdatedStakingAccount {
	fn from(value: &StakingAccount) -> Self {
		Self { account: value.clone() }
	}
}

impl Into<StakingAccount> for &UpdatedStakingAccount {
	fn into(self) -> StakingAccount {
		self.account.clone()
	}
}

#[derive(Debug, Clone)]
struct UpdatedTransactionMetadata {
	metadata: TransactionMetadata,
}

impl Updatable<UpdatedTransactionMetadata> for UpdatedTransactionMetadata {
	fn update(&mut self, v: &UpdatedTransactionMetadata) {
		self.metadata = v.metadata.clone()
	}
}

impl From<&TransactionMetadata> for UpdatedTransactionMetadata {
	fn from(value: &TransactionMetadata) -> Self {
		Self {
			metadata: value.clone()
		}
	}
}

impl From<TransactionMetadata> for UpdatedTransactionMetadata {
	fn from(value: TransactionMetadata) -> Self {
		Self::from(&value)
	}
}

impl Into<TransactionMetadata> for &UpdatedTransactionMetadata {
	fn into(self) -> TransactionMetadata {
		self.metadata.clone()
	}
}

#[derive(Clone)]
pub struct UpdatedState {
	db_channel: tokio::sync::mpsc::Sender<DBRequest>,
	accounts: RefCell<HashMap<Address, CachedStatus<UpdatedAccount>>>,
	contracts: RefCell<HashMap<Address, CachedStatus<UpdatedContract>>>,
	contract_instances: RefCell<HashMap<Address, CachedStatus<UpdatedContractInstance>>>,
	contract_states: RefCell<
		HashMap<Address, HashMap<ContractInstanceKey, CachedStatus<UpdatedContractInstanceValue>>>,
	>,
	contract_events: RefCell<Vec<CachedStatus<UpdatedEvent>>>,
	staking_pools: RefCell<HashMap<Address, CachedStatus<UpdatedStakingPool>>>,
	staking_accounts: RefCell<HashMap<Address, HashMap<Address, CachedStatus<UpdatedStakingAccount>>>>,
	transaction_metadatas: RefCell<Vec<CachedStatus<UpdatedTransactionMetadata>>>,
}

impl UpdatedState {
	pub fn new(db_channel: tokio::sync::mpsc::Sender<DBRequest>) -> Self {
		Self {
			db_channel,
			accounts: RefCell::new(HashMap::new()),
			contracts: RefCell::new(HashMap::new()),
			contract_instances: RefCell::new(HashMap::new()),
			contract_states: RefCell::new(HashMap::new()),
			contract_events: RefCell::new(Vec::new()),
			staking_pools: RefCell::new(HashMap::new()),
			staking_accounts: RefCell::new(HashMap::new()),
			transaction_metadatas: RefCell::new(Vec::new()),
		}
	}

	pub fn merge(&mut self, another_state: &Self) {
		// Accounts
		another_state.accounts.borrow().iter().for_each(|(k, v)| {
			self.accounts.borrow_mut().insert(k.clone(), v.clone());
		});

		// Contracts
		another_state.contracts.borrow().iter().for_each(|(k, v)| {
			self.contracts.borrow_mut().insert(k.clone(), v.clone());
		});

		// Contarct instances
		another_state.contract_instances.borrow().iter().for_each(|(k, v)| {
			self.contract_instances.borrow_mut().insert(k.clone(), v.clone());
		});

		// Contract states
		another_state.contract_states.borrow().iter().for_each(|(instance_address, another_key_values)| {
			self.contract_states.borrow_mut()
				.entry(*instance_address)
				.and_modify(|key_values| {
					key_values.extend(another_key_values.iter()
						.map(|(k, v)| (k.clone(), v.clone()))
					);
				})
				.or_insert(another_key_values.clone());
		});
		
		// Contract events
		*self.contract_events.borrow_mut() = another_state.contract_events.borrow().clone();
		
		// Staking pools
		another_state.staking_pools.borrow().iter().for_each(|(pool_address, pool)| {
			self.staking_pools.borrow_mut().insert(*pool_address, pool.clone());
		});

		// Staking accounts
		another_state.staking_accounts.borrow().iter().for_each(|(pool_address, staking_accounts)| {
			self.staking_accounts.borrow_mut()
				.entry(*pool_address)
				.and_modify(|accounts| {
					accounts.extend(staking_accounts.iter().map(|(k, v)| {
						(k.clone(), v.clone())
					}));
				})
				.or_insert(staking_accounts.clone());
		});
		
		// Transaction metadatas
		*self.transaction_metadatas.borrow_mut() = another_state.transaction_metadatas.borrow().clone();
	}

	pub async fn commit<'a>(&self, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		let account_state = account::account_state::AccountState::new(db_pool_conn).await?;
		for (_address, cache_record) in self.accounts.borrow().iter() {
			match cache_record {
				CachedStatus::Updated(Some(account)) => {
					account_state.update_account(&account.account).await?;
				},
				CachedStatus::Updated(None) => Err(anyhow!("Should never happen: Account is None"))?,
				CachedStatus::Cached(_) => ()
			}
		}

		let contract_state = contract::contract_state::ContractState::new(db_pool_conn).await?;
		for (_address, cache_record) in self.contracts.borrow().iter() {
			match cache_record {
				CachedStatus::Updated(Some(contract)) => {
					contract_state.store_contract(&contract.into()).await?
				},
				CachedStatus::Updated(None) => Err(anyhow!("Should never happen: Contract is None"))?,
				CachedStatus::Cached(_) => ()
			}
		}

		let contract_instance_state =
			contract_instance::contract_instance_state::ContractInstanceState::new(db_pool_conn)
				.await?;
		for (_address, cache_record) in self.contract_instances.borrow().iter() {
			match cache_record {
				CachedStatus::Updated(Some(contract_instance)) => {
					contract_instance_state
						.store_contract_instance(&contract_instance.into())
						.await?;
				},
				CachedStatus::Updated(None) => Err(anyhow!("Should never happen: ContractInstance is None"))?,
				CachedStatus::Cached(_) => ()
			}
		}

		for (instance_address, key_values) in self.contract_states.borrow().iter() {
			for (key, cache_value) in key_values.iter() {
				match cache_value {
					CachedStatus::Updated(Some(value)) => {
						if let Some(value) = &value.value {
							contract_instance_state
								.store_state_key_value(instance_address, key, value)
								.await?;
						} else {
							contract_instance_state
								.delete_state_key_value(instance_address, key)
								.await?;
						}
					},
					CachedStatus::Updated(None) => {
						contract_instance_state
							.delete_state_key_value(instance_address, key)
							.await?;
					},
					CachedStatus::Cached(_) => (),
				}
			}
		}

		let event_state = event::event_state::EventState::new(db_pool_conn).await?;
		for cache_event in self.contract_events.borrow().iter() {
			match cache_event {
				CachedStatus::Updated(Some(event)) => {
					event_state.create_event(&event.into()).await?;
				},
				_ => (),
			}
		}

		let staking_state = staking::staking_state::StakingState::new(&db_pool_conn).await?;
		for (pool_address, cache_pool) in self.staking_pools.borrow().iter() {
			match cache_pool {
				CachedStatus::Updated(Some(pool)) => {
					if staking_state.is_staking_pool_exists(pool_address).await? {
						staking_state.update_staking_pool_block_number(pool_address, pool.pool.updated_block_number).await?;
						if let Some(contract_instance_address) = &pool.pool.contract_instance_address {
							staking_state.update_contract(contract_instance_address, pool_address, pool.pool.updated_block_number).await?;
						}
					} else {
						staking_state.insert_staking_pool(&pool.pool).await?;
					}
				},
				_ => ()
			}
		}

		for (_pool_address, staking_accounts) in self.staking_accounts.borrow().iter() {
			for (_staking_account_address, cache_staking_account) in staking_accounts.iter() {
				match cache_staking_account {
					CachedStatus::Updated(Some(staking_account)) => {
						if staking_state.is_staking_account_exists(&staking_account.account.account_address, &staking_account.account.pool_address).await? {
							staking_state.update_staking_account(&staking_account.account).await?;
						} else {
							staking_state.insert_staking_account(
								&staking_account.account.account_address,
								&staking_account.account.pool_address,
								staking_account.account.balance
							).await?;
						}
					},
					_ => ()
				}
			}
		}

		let block_state = block::block_state::BlockState::new(db_pool_conn).await?;
		for cache_tx_metadata in self.transaction_metadatas.borrow().iter() {
			match cache_tx_metadata {
				CachedStatus::Updated(Some(metadata)) => {
					block_state.store_transaction_metadata(metadata.into()).await?;
				}
				_ => ()
			}
		}

		Ok(())
	}

	pub fn is_contract_state_updated(&self, contract_instance_address: &Address) -> bool {
		if let Some(map) = self.contract_states.borrow().get(contract_instance_address) {
			map.values().any(|v| {
				matches!(v, CachedStatus::Updated(_))
			})
		} else {
			false
		}
	}

	pub fn get_account(&self, address: &Address) -> Result<Account, Error> {
		if let Some(cached) = self.accounts.borrow().get(address) {
			let account = cached
				.get_inner()
				.ok_or(anyhow!("Account is removed, account: {}", hex::encode(address)))?
				.account
				.clone();
			return Ok(account);
		}

		let (tx, rx) = tokio::sync::oneshot::channel();
		self.db_channel
			.blocking_send(DBRequest::GetAccount { address: address.clone(), response: tx })?;
		match rx.blocking_recv()? {
			Ok(account) => {
				self.accounts
					.borrow_mut()
					.insert(*address, CachedStatus::Cached((&account).into()));
				Ok(account)
			},
			Err(e) => Err(e),
		}
	}

	pub fn update_account(&mut self, account: &Account) -> Result<(), Error> {
		self.accounts
			.borrow_mut()
			.insert(account.address, CachedStatus::Updated(Some((account).into())));
		Ok(())
	}

	pub fn get_nonce(&self, address: &Address) -> Result<Nonce, Error> {
		Ok(self.get_account(address)?.nonce)
	}

	pub fn increment_nonce(&mut self, address: &Address) -> Result<(), Error> {
		let mut account = self.get_account(address)?;
		account.nonce = account.nonce.checked_add(1).ok_or(anyhow!(
			"Nonce overflow, address: {}, nonce: {}",
			hex::encode(address),
			account.nonce
		))?;

		self.update_account(&account)?;

		Ok(())
	}

	pub fn get_balance(&self, address: &Address) -> Result<Balance, Error> {
		Ok(self.get_account(address)?.balance)
	}

	pub fn mint(&mut self, amount: Balance, to_address: &Address) -> Result<(), Error> {
		let mut to_account = self.get_account(to_address)?;
		to_account.balance = to_account.balance.checked_add(amount).ok_or(anyhow!(
			"Mint: balance owerflow, account: {}, balance: {}, amount: {}",
			hex::encode(to_address),
			to_account.balance,
			amount
		))?;

		self.update_account(&to_account)?;

		Ok(())
	}

	pub fn transfer(
		&mut self,
		from_address: &Address,
		to_address: &Address,
		amount: Balance,
	) -> Result<(), Error> {
		let mut from_account = self.get_account(from_address)?;

		info!("DEBUB::TMP_27012025: BEFORE Transfer: from_account.balance: {}, amount: {}", from_account.clone().balance, amount.clone());
		from_account.balance = from_account.balance.checked_sub(amount).ok_or(anyhow!(
			"Transfer: Can't decrease balance, account: {}, balance: {}, required balance: {}",
			hex::encode(from_address),
			from_account.balance,
			amount
		))?;
		info!("DEBUB::TMP_27012025: AFTER Transfer: from_account.balance: {}, amount: {}", from_account.clone().balance, amount.clone());
		
		// Get an account or create a new one
		let mut to_account = match self.get_account(to_address) {
			Ok(account) => {
				if(from_address == to_address) {
					from_account.clone()
				}
				else{
					account
				}
			},
			Err(_) => {
				let account = Account {
					address: *to_address,
					balance: 0,
					nonce: 0,
					account_type: system::account::AccountType::User,
				};
				self.update_account(&account)?;
				account
			}
		};
		info!("DEBUB::TMP_27012025: BEFORE Transfer: to_account.balance: {}, amount: {}", to_account.clone().balance, amount.clone());
		to_account.balance = to_account.balance.checked_add(amount).ok_or(anyhow!(
			"Transfer: balance overflow, account: {}, balance: {}, amount: {}",
			hex::encode(to_address),
			to_account.balance,
			amount
		))?;
		info!("DEBUB::TMP_27012025: AFTER Transfer: to_account.balance: {}, amount: {}", to_account.clone().balance, amount.clone());
		self.update_account(&from_account).map_err(|e| {
			anyhow!("Can't update 'from' account {}, error: {}", hex::encode(&from_address), e)
		})?;
		self.update_account(&to_account).map_err(|e| {
			anyhow!("Can't update 'to' account {}, error: {}", hex::encode(&to_address), e)
		})?;

		Ok(())
	}

	pub fn create_account(&mut self, account: &Account) -> Result<(), Error> {
		if self.is_valid_account(&account.address)? {
			Err(anyhow!("Account address collision, address: {}", hex::encode(account.address)))?
		}
		self.update_account(account)?;
		Ok(())
	}

	pub fn create_system_account(&mut self, address: &Address) -> Result<(), Error> {
		let account = Account::new_system(*address);
		self.create_account(&account)
	}

	pub fn is_valid_account(&self, address: &Address) -> Result<bool, Error> {
		// TODO: optimize
		Ok(self.get_account(address).is_ok())
	}

	pub fn create_event(&mut self, event: &Event) -> Result<(), Error> {
		self.contract_events
			.borrow_mut()
			.push(CachedStatus::Updated(Some(UpdatedEvent::from(event))));
		Ok(())
	}

	pub fn get_contract(&self, address: &Address) -> Result<Contract, Error> {
		if let Some(cached) = self.contracts.borrow().get(address) {
			let contract = cached
				.get_inner()
				.ok_or(anyhow!("Contract is removed, address: {}", hex::encode(address)))?;
			return Ok(contract.into());
		}

		let (tx, rx) = tokio::sync::oneshot::channel();
		self.db_channel
			.blocking_send(DBRequest::GetContract { address: address.clone(), response: tx })?;
		match rx.blocking_recv()? {
			Ok(contract) => {
				self.contracts
					.borrow_mut()
					.entry(*address)
					.and_modify(|record| {
						record.update_cached(&UpdatedContract::from(contract.clone()));
					})
					.or_insert(CachedStatus::new_cached(UpdatedContract::from(contract.clone())));
				Ok(contract)
			},
			Err(e) => Err(e),
		}
	}

	pub fn store_contract(&mut self, contract: &Contract) -> Result<(), Error> {
		// TODO: Check collision

		self.contracts
			.borrow_mut()
			.entry(contract.address)
			.and_modify(|record| {
				record.update(Some(UpdatedContract::from(contract)));
			})
			.or_insert(CachedStatus::new_updated(Some(UpdatedContract::from(contract))));

		Ok(())
	}

	pub fn get_contract_owner(&self, address: &Address) -> Result<(AccessType, Address), Error> {
		// TODO: Optimize

		let contract = self.get_contract(&address)?;
		Ok((contract.access, contract.owner_address))
	}

	pub fn is_valid_contract_instance(&self, instance_address: &Address) -> Result<bool, Error> {
		// TODO: Optimize

		Ok(self.get_contract_instance(instance_address).is_ok())
	}

	pub fn get_contract_instance(
		&self,
		instance_address: &Address,
	) -> Result<ContractInstance, Error> {
		if let Some(cached) = self.contract_instances.borrow().get(instance_address) {
			let contract_instance = cached.get_inner().ok_or(anyhow!(
				"Contract instance is removed, address: {}",
				hex::encode(instance_address)
			))?;
			return Ok(contract_instance.into());
		}

		let (tx, rx) = tokio::sync::oneshot::channel();
		self.db_channel.blocking_send(DBRequest::GetContractInstance {
			address: instance_address.clone(),
			response: tx,
		})?;
		match rx.blocking_recv()? {
			Ok(contract_instance) => {
				self.contract_instances
					.borrow_mut()
					.entry(*instance_address)
					.and_modify(|record| {
						record.update_cached(&UpdatedContractInstance::from(
							contract_instance.clone(),
						));
					})
					.or_insert(CachedStatus::new_cached(UpdatedContractInstance::from(
						contract_instance.clone(),
					)));
				Ok(contract_instance)
			},
			Err(e) => Err(e),
		}
	}

	pub fn store_contract_instance(
		&mut self,
		contract_instance: &ContractInstance,
	) -> Result<(), Error> {
		// TODO: Check collision

		self.contract_instances
			.borrow_mut()
			.entry(contract_instance.instance_address)
			.and_modify(|record| {
				record.update(Some(UpdatedContractInstance::from(contract_instance)));
			})
			.or_insert(CachedStatus::new_updated(Some(UpdatedContractInstance::from(contract_instance))));

		Ok(())
	}

	pub fn get_state_key_value(
		&self,
		instance_address: &Address,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error> {
		if let Some(cached_values) = self.contract_states.borrow().get(instance_address) {
			if let Some(cached_value) = cached_values.get(key) {
				return Ok(cached_value.get_inner().and_then(|v| v.value.clone()));
			}
		}

		let (tx, rx) = tokio::sync::oneshot::channel();
		self.db_channel.blocking_send(DBRequest::GetContractInstanceValue {
			address: instance_address.clone(),
			key: key.clone(),
			response: tx,
		})?;
		match rx.blocking_recv()? {
			Ok(value) => {
				self.contract_states
					.borrow_mut()
					.entry(*instance_address)
					.and_modify(|record| {
						record
							.entry(key.clone())
							.and_modify(|record| {
								let val = UpdatedContractInstanceValue { value: value.clone() };
								record.update_cached(&val);
							})
							.or_insert(CachedStatus::new_cached(UpdatedContractInstanceValue {
								value: value.clone(),
							}));
					})
					.or_insert(HashMap::from([(
						key.clone(),
						CachedStatus::new_cached(UpdatedContractInstanceValue {
							value: value.clone(),
						}),
					)]));
				Ok(value)
			},
			Err(e) => Err(e),
		}
	}

	pub fn store_state_key_value(
		&mut self,
		instance_address: &Address,
		key: &ContractInstanceKey,
		value: &ContractInstanceValue,
	) -> Result<(), Error> {
		self.contract_states
			.borrow_mut()
			.entry(*instance_address)
			.and_modify(|record| {
				record.insert(
					key.clone(),
					CachedStatus::new_updated(Some(UpdatedContractInstanceValue {
						value: Some(value.clone()),
					})),
				);
			})
			.or_insert(HashMap::from([(
				key.clone(),
				CachedStatus::new_updated(Some(UpdatedContractInstanceValue {
					value: Some(value.clone()),
				})),
			)]));

		Ok(())
	}

	pub fn delete_state_key_value(
		&mut self,
		instance_address: &Address,
		key: &ContractInstanceKey,
	) -> Result<(), Error> {
		self.contract_states
			.borrow_mut()
			.entry(*instance_address)
			.and_modify(|record| {
				record.insert(key.clone(), CachedStatus::new_updated(None));
			})
			.or_insert(HashMap::from([(key.clone(), CachedStatus::new_updated(None))]));

		Ok(())
	}

	pub fn create_pool(
		&mut self,
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
	) -> Result<Address, Error> {
		let pool_address = Account::pool_address(account_address, cluster_address, nonce);
		self.create_system_account(&pool_address)?;

		let pool = StakingPool::new(
			&pool_address,
			account_address,
			contract_instance_address,
			cluster_address,
			created_block_number,
			min_stake,
			max_stake,
			min_pool_balance,
			max_pool_balance,
			staking_period,
		);

		self.staking_pools
			.borrow_mut()
			.insert(pool_address, CachedStatus::Updated(Some((UpdatedStakingPool { pool }).into())));
		
		Ok(pool_address)
	}

	pub fn get_staking_pool(&self, pool_address: &Address) -> Result<StakingPool, Error> {
		if let Some(cached) = self.staking_pools.borrow().get(pool_address) {
			let pool = cached.get_inner().ok_or(anyhow!(
				"Staking Pool is removed, address: {}",
				hex::encode(pool_address)
			))?;
			return Ok(pool.into());
		}

		let (tx, rx) = tokio::sync::oneshot::channel();
		self.db_channel.blocking_send(DBRequest::GetStakingPool {
			address: pool_address.clone(),
			response: tx,
		})?;
		match rx.blocking_recv()? {
			Ok(pool) => {
				self.staking_pools
					.borrow_mut()
					.entry(*pool_address)
					.and_modify(|record| {
						record.update_cached(&UpdatedStakingPool::from(
							&pool,
						));
					})
					.or_insert(CachedStatus::new_cached(UpdatedStakingPool::from(
						&pool,
					)));
				Ok(pool)
			},
			Err(e) => Err(e),
		}
	}

	pub fn get_staking_account(&self, account_address: &Address, pool_address: &Address) -> Result<StakingAccount, Error> {
		if let Some(pool_accounts) = self.staking_accounts.borrow().get(pool_address) {
			if let Some(cached) = pool_accounts.get(account_address) {
				let pool = cached.get_inner().ok_or(anyhow!(
					"Staking Pool is removed, address: {}",
					hex::encode(pool_address)
				))?;	
				return Ok(pool.into());
			}
		}

		let (tx, rx) = tokio::sync::oneshot::channel();
		self.db_channel.blocking_send(DBRequest::GetStakingAccount {
			pool_address: pool_address.clone(),
			account_address: account_address.clone(),
			response: tx,
		})?;
		match rx.blocking_recv()? {
			Ok(staking_account) => {
				self.staking_accounts
					.borrow_mut()
					.entry(*pool_address)
					.and_modify(|pool_accounts| {
						pool_accounts.entry(*account_address).and_modify(|record| {
							record.update_cached(&UpdatedStakingAccount::from(
								&staking_account,
							));
						})
						.or_insert(CachedStatus::new_cached(UpdatedStakingAccount::from(
							&staking_account,
						)));
					})
					.or_insert(HashMap::from([(*account_address, CachedStatus::new_cached(UpdatedStakingAccount::from(
						&staking_account,
					)))]));
		
				Ok(staking_account)
			},
			Err(e) => Err(e),
		}
	}

	pub fn is_staking_account_exists(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<bool, Error> {
		Ok(self.get_staking_account(account_address, pool_address).is_ok())
	}

	pub fn update_staking_account(&mut self, staking_account: &StakingAccount) -> Result<(), Error> {
		self.staking_accounts.borrow_mut().entry(staking_account.pool_address)
		.and_modify(|staking_accounts| {
			staking_accounts.insert(staking_account.account_address, CachedStatus::Updated(Some(UpdatedStakingAccount::from(staking_account))));
		})
		.or_insert(HashMap::from([(staking_account.account_address, CachedStatus::new_updated(Some(UpdatedStakingAccount::from(staking_account))))]));

		Ok(())
	}

	pub fn insert_staking_account(
		&mut self,
		account_address: &Address,
		pool_address: &Address,
		balance: Balance,
	) -> Result<(), Error> {
		let staking_account = StakingAccount {
			account_address: *account_address,
			pool_address: *pool_address,
			balance,
		};

		self.update_staking_account(&staking_account)
	}

	pub fn update_staking_pool_block_number(&mut self, pool_address: &Address, updated_block_number: BlockNumber) -> Result<(), Error> {
		let mut pool = self.get_staking_pool(pool_address)?;
		pool.updated_block_number = updated_block_number;

		self.staking_pools.borrow_mut().insert(*pool_address, CachedStatus::new_updated(Some(pool.into())));
		Ok(())
	}

	// Reference: staking/src/staking_manager.rs - `stake`
	pub fn stake(
		&mut self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
	) -> Result<(), Error> {
		let pool = self.get_staking_pool(pool_address)?;
		if let Some(min_stake) = pool.min_stake {
			if min_stake > amount {
				Err(anyhow!("Stake: amount ({}) is less than min stake ({})", amount, min_stake))?;
			}
		}
		if let Some(max_stake) = pool.max_stake {
			if max_stake < amount {
				Err(anyhow!("Stake: amount ({}) is greater than max stake ({})", amount, max_stake))?;
			}
		}
		if let Some(max_pool_balance) = pool.max_pool_balance {
			let pool_balance = self.get_balance(pool_address)?;
			if max_pool_balance.saturating_sub(amount) < pool_balance {
				Err(anyhow!("Stake: reached maximal pool balance ({}), balance: {}, amount: {}", max_pool_balance, pool_balance, amount))?;
			}
		}

		// If transfer fails then there is not enough fund
		self.transfer(account_address, pool_address, amount)?;

		let mut staking_account = self.get_staking_account(account_address, pool_address).unwrap_or(StakingAccount::new(*account_address, *pool_address));
		staking_account.balance = staking_account.balance.checked_add(amount)
			.ok_or(anyhow!("Stake: balance overflow, staking account: {}, balance: {}, amount: {}", 
				hex::encode(staking_account.account_address), staking_account.balance, amount))?;
		
		self.update_staking_account(&staking_account)?;
		self.update_staking_pool_block_number(pool_address, block_number)?;

		Ok(())
	}

	pub fn un_stake(
		&mut self,
		pool_address: &Address,
		account_address: &Address,
		block_number: BlockNumber,
		amount: Balance,
	) -> Result<(), Error> {
		let mut staking_account = self.get_staking_account(account_address, pool_address)?;
		staking_account.balance = staking_account.balance.checked_sub(amount)
			.ok_or(anyhow!("Unstake: no enough fund to unstake, account: {}, staked amount: {}, amount: {}", 
				hex::encode(account_address), staking_account.balance, amount))?;

		self.update_staking_account(&staking_account)?;
		self.update_staking_pool_block_number(pool_address, block_number)?;
		
		self.transfer(pool_address, account_address, amount)?;
		
		Ok(())
	}

	pub fn update_contract(
		&mut self,
		contract_instance_address: &Address,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error> {
		let mut pool = self.get_staking_pool(pool_address)?;
		
		pool.contract_instance_address = Some(*contract_instance_address);
		pool.updated_block_number = block_number;
		self.staking_pools.borrow_mut().insert(*pool_address, CachedStatus::new_updated(Some(pool.into())));

		Ok(())
	}

	pub fn insert_staking_pool(&mut self, pool: &StakingPool) -> Result<(), Error> {
		self.staking_pools.borrow_mut().insert(pool.pool_address, CachedStatus::new_updated(Some(pool.into())));
		Ok(())
	}

	pub fn store_transaction_metadata(
		&mut self,
		metadata: TransactionMetadata) -> Result<(), Error> {
			self.transaction_metadatas.borrow_mut().push(CachedStatus::Updated(Some(metadata.into())));
			Ok(())
		}
}
