use anyhow::Error;
use async_trait::async_trait;
use db_traits::{account::AccountState, base::BaseState};
use primitives::{Address, Balance, Nonce};
use rocksdb::DB;
use system::account::Account;

#[allow(dead_code)]
/// Represents the state implementation for RocksDB storage.
pub struct StateRock {
	/// The path to the RocksDB database.
	pub(crate) db_path: String,
	/// The RocksDB instance.
	pub db: DB,
}

#[async_trait]
impl BaseState<Account> for StateRock {
	/// Table creation is not required in RocksDB
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	/// Inserts a new account into the RocksDB database.
	async fn create(&self, account: &Account) -> Result<(), Error> {
		let key = format!("account:{:?}", account.address);
		let value = serde_json::to_string(account)?;
		// Insert the account into the database
		self.db.put(key, value)?;
		Ok(())
	}

	/// Updates an existing account in the RocksDB database.
	async fn update(&self, account: &Account) -> Result<(), Error> {
		let key = format!("account:{:?}", account.address);
		let value = serde_json::to_string(account)?;
		// Update the account in the database
		self.db.put(key, value)?;
		Ok(())
	}

	async fn raw_query(&self, _query: &str) -> Result<(), Error> {
		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implement setting schema version in RocksDB here
		Ok(())
	}
}

#[async_trait]
impl AccountState for StateRock {
	/// Updates the balance of an existing account in the RocksDB database.
	///
	/// # Arguments
	///
	/// * `account` - A reference to the `Account` struct containing the updated balance.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A result indicating success or failure, with an `Error` type
	/// containing details if an error occurs during the operation.
	///
	/// # Errors
	///
	/// Returns an error if there is an issue updating the balance or retrieving the account
	/// from the database.
	async fn update_balance(&self, account: &Account) -> Result<(), Error> {
		// Construct the key for the account in the database
		let key = format!("account:{:?}", account.address);
		// Retrieve the existing account from the database
		let mut account_update: Account = self.get_account(&account.address).await?;
		// Update the balance of the account
		account_update.balance = account.balance;
		// Serialize the updated account to JSON
		let value = serde_json::to_string(&account_update)?;
		// Update the account in the database
		self.db.put(&key, value)?;
		Ok(())
	}

	/// Increments the nonce of an account in the RocksDB database.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` of the account whose nonce will be incremented.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A result indicating success or failure, with an `Error` type
	/// containing details if an error occurs during the operation.
	///
	/// # Errors
	///
	/// Returns an error if there is an issue updating the nonce or retrieving the account
	/// from the database.
	async fn increment_nonce(&self, address: &Address) -> Result<(), Error> {
		// Construct the key for the account in the database
		let key = format!("account:{:?}", address);
		// Retrieve the existing account from the database
		let mut account: Account = self.get_account(address).await?;
		// Increment the nonce of the account
		account.nonce += 1;
		// Serialize the updated account to JSON
		let value = serde_json::to_string(&account)?;
		// Update the account in the database
		self.db.put(&key, value)?;
		Ok(())
	}

	/// Retrieves an account from the RocksDB database based on the given address.
	///
	/// If the account does not exist in the database, it will create a new account
	/// with the provided address and return it.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` of the account to retrieve.
	///
	/// # Returns
	///
	/// * `Result<Account, Error>` - A result containing the retrieved account if
	/// it exists in the database, or an error if the account does not exist or
	/// if there is an issue retrieving it from the database.
	///
	/// # Errors
	///
	/// Returns an error if there is an issue retrieving or creating the account
	/// in the database.
	async fn get_account(&self, address: &Address) -> Result<Account, Error> {
		// Construct the key for the account in the database
		let key = format!("account:{:?}", address);
		// Check if the account exists in the database
		if !self.is_valid_account(address).await? {
			// If the account does not exist, create a new account with the provided address
			let account = Account::new(*address);
			self.create(&account).await?; // Create the account in the database
		}

		// Retrieve the account from the database
		match self.db.get(&key)? {
			Some(value) => {
				// Deserialize the retrieved account from JSON
				let value_str = String::from_utf8_lossy(&value);
				Ok(serde_json::from_str(&value_str)?)
			},
			None => Err(Error::msg("Account not found")), // Account not found in the database
		}
	}

	/// Retrieves the nonce of an account from the RocksDB database based on the given address.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` of the account.
	///
	/// # Returns
	///
	/// * `Result<Nonce, Error>` - A result containing the nonce of the account if it exists
	/// in the database, or an error if the account does not exist or if there is an issue
	/// retrieving the nonce.
	///
	/// # Errors
	///
	/// Returns an error if there is an issue retrieving the nonce or if the account does not
	/// exist in the database.
	async fn get_nonce(&self, address: &Address) -> Result<Nonce, Error> {
		// Retrieve the account from the database
		let account: Account = self.get_account(address).await?;
		// Return the nonce of the account
		Ok(account.nonce)
	}

	/// Retrieves the balance of an account from the RocksDB database based on the given address.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` of the account.
	///
	/// # Returns
	///
	/// * `Result<Balance, Error>` - A result containing the balance of the account if it exists
	/// in the database, or an error if the account does not exist or if there is an issue
	/// retrieving the balance.
	///
	/// # Errors
	///
	/// Returns an error if there is an issue retrieving the balance or if the account does not
	/// exist in the database.
	async fn get_balance(&self, address: &Address) -> Result<Balance, Error> {
		// Retrieve the account from the database
		let account = self.get_account(address).await?;
		// Return the balance of the account
		Ok(account.balance)
	}

	/// Checks if an account with the given address exists in the RocksDB database.
	///
	/// # Arguments
	///
	/// * `address` - A reference to the `Address` of the account.
	///
	/// # Returns
	///
	/// * `Result<bool, Error>` - A result indicating whether the account exists in the
	/// database (`true`) or not (`false`), or an error if there is an issue checking
	/// the existence of the account.
	///
	/// # Errors
	///
	/// Returns an error if there is an issue checking the existence of the account.
	async fn is_valid_account(&self, address: &Address) -> Result<bool, Error> {
		// Construct the key for the account in the database
		let key = format!("account:{:?}", address);
		// Check if the account exists in the database
		Ok(self.db.get(key)?.is_some())
	}
}
