use crate::{state_cas::StateCas, state_pg::StatePg, state_rock::StateRock};
use anyhow::Error;
use db::db::DbTxConn;
use db_traits::{account::AccountState as AccountStateInternal, base::BaseState};
use primitives::{Address, Balance, Nonce};
use rocksdb::DB;
use std::sync::Arc;
use system::account::Account;

/// Enum representing different internal database state implementations.
pub enum StateInternalImpl<'a> {
	/// State implementation using RocksDB.
	StateRock(StateRock),
	/// State implementation using PostgreSQL.
	StatePg(StatePg<'a>),
	/// State implementation using Cassandra.
	StateCas(StateCas),
}

/// Struct representing the account state, managing interactions with the underlying database.
pub struct AccountState<'a> {
	/// The internal database state.
	pub state: Arc<StateInternalImpl<'a>>,
}

impl<'a> AccountState<'a> {
	/// Creates a new `AccountState` instance based on the provided database connection.
	///
	/// # Arguments
	///
	/// * `db_pool_conn` - The database connection.
	///
	/// # Returns
	///
	/// * `Result<AccountState, Error>` - A `Result` containing either the initialized
	///   `AccountState` or an error.
	pub async fn new(db_pool_conn: &'a DbTxConn<'a>) -> Result<Self, Error> {
		let state: StateInternalImpl<'a> = match &db_pool_conn {
			DbTxConn::POSTGRES(pg) => StateInternalImpl::StatePg(StatePg { pg }),
			DbTxConn::CASSANDRA(session) =>
				StateInternalImpl::StateCas(StateCas { session: session.clone() }),
			DbTxConn::ROCKSDB(db_path) => {
				let db_path = format!("{}/account", db_path);
				StateInternalImpl::StateRock(StateRock {
					db_path: db_path.clone(),
					db: DB::open_default(db_path)?,
				})
			},
		};

		let state = AccountState { state: Arc::new(state) };

		state.create_table().await?;
		Ok(state)
	}

	/// Creates a table in the database for storing account-related data.
	///
	/// This method delegates the creation of the table to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A `Result` indicating success or failure of the table creation
	///   operation.
	pub async fn create_table(&self) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create_table().await,
			StateInternalImpl::StatePg(s) => s.create_table().await,
			StateInternalImpl::StateCas(s) => s.create_table().await,
		}
	}

	/// Creates a new account in the database.
	///
	/// This method delegates the creation of the account to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `account` - A reference to the `Account` struct representing the account to be created.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A `Result` indicating success or failure of the account creation
	///   operation.
	pub async fn create_account(&self, account: &Account) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.create(account).await,
			StateInternalImpl::StatePg(s) => s.create(account).await,
			StateInternalImpl::StateCas(s) => s.create(account).await,
		}
	}

	/// Executes a raw SQL query on the database.
	///
	/// This method delegates the execution of the raw query to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `query` - A string slice containing the raw SQL query to be executed.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A `Result` indicating success or failure of the raw query execution.
	pub async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.raw_query(query).await,
			StateInternalImpl::StatePg(s) => s.raw_query(query).await,
			StateInternalImpl::StateCas(s) => s.raw_query(query).await,
		}
	}

	/// Updates an existing account in the database.
	///
	/// This method delegates the account update operation to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `account` - A reference to the `Account` struct representing the account to be updated.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A `Result` indicating success or failure of the account update
	///   operation.
	pub async fn update_account(&self, account: &Account) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.update(account).await,
			StateInternalImpl::StatePg(s) => s.update(account).await,
			StateInternalImpl::StateCas(s) => s.update(account).await,
		}
	}

	/// Updates the balance of an account in the database.
	///
	/// This method delegates the balance update operation to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `account` - A reference to the `Account` struct representing the account whose balance is
	///   to be updated.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A `Result` indicating success or failure of the balance update
	///   operation.
	pub async fn update_balance(&self, account: &Account) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.update_balance(account).await,
			StateInternalImpl::StatePg(s) => s.update_balance(account).await,
			StateInternalImpl::StateCas(s) => s.update_balance(account).await,
		}
	}

	/// Increments the nonce of an account in the database.
	///
	/// This method delegates the nonce increment operation to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` struct representing the address of the account
	///   whose nonce is to be incremented.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A `Result` indicating success or failure of the nonce increment
	///   operation.
	pub async fn increment_nonce(&self, address: &Address) -> Result<(), Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.increment_nonce(address).await,
			StateInternalImpl::StatePg(s) => s.increment_nonce(address).await,
			StateInternalImpl::StateCas(s) => s.increment_nonce(address).await,
		}
	}

	/// Retrieves an account from the database based on its address.
	///
	/// This method delegates the retrieval operation to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` struct representing the address of the account to
	///   retrieve.
	///
	/// # Returns
	///
	/// * `Result<Account, Error>` - A `Result` containing the retrieved `Account` if it exists, or
	///   an `Error` if the operation fails.
	pub async fn get_account(&self, address: &Address) -> Result<Account, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_account(address).await,
			StateInternalImpl::StatePg(s) => s.get_account(address).await,
			StateInternalImpl::StateCas(s) => s.get_account(address).await,
		}
	}

	/// Retrieves the nonce of an account from the database based on its address.
	///
	/// This method delegates the retrieval operation to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` struct representing the address of the account to
	///   retrieve the nonce for.
	///
	/// # Returns
	///
	/// * `Result<Nonce, Error>` - A `Result` containing the retrieved `Nonce` if the account
	///   exists, or an `Error` if the operation fails.
	pub async fn get_nonce(&self, address: &Address) -> Result<Nonce, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_nonce(address).await,
			StateInternalImpl::StatePg(s) => s.get_nonce(address).await,
			StateInternalImpl::StateCas(s) => s.get_nonce(address).await,
		}
	}

	/// Retrieves the balance of an account from the database based on its address.
	///
	/// This method delegates the retrieval operation to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` struct representing the address of the account to
	///   retrieve the balance for.
	///
	/// # Returns
	///
	/// * `Result<Balance, Error>` - A `Result` containing the retrieved `Balance` if the account
	///   exists, or an `Error` if the operation fails.
	pub async fn get_balance(&self, address: &Address) -> Result<Balance, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.get_balance(address).await,
			StateInternalImpl::StatePg(s) => s.get_balance(address).await,
			StateInternalImpl::StateCas(s) => s.get_balance(address).await,
		}
	}

	/// Checks if an account with the given address exists in the database.
	///
	/// This method delegates the existence check operation to the appropriate database state
	/// implementation based on the internal state of the `AccountState` instance.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` struct representing the address of the account to
	///   check.
	///
	/// # Returns
	///
	/// * `Result<bool, Error>` - A `Result` containing a boolean value indicating whether the
	///   account exists (`true`) or not (`false`), or an `Error` if the operation fails.
	pub async fn is_valid_account(&self, address: &Address) -> Result<bool, Error> {
		match &*self.state {
			StateInternalImpl::StateRock(s) => s.is_valid_account(address).await,
			StateInternalImpl::StatePg(s) => s.is_valid_account(address).await,
			StateInternalImpl::StateCas(s) => s.is_valid_account(address).await,
		}
	}
}
