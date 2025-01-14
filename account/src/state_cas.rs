use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db::utils::{FromByteArray, ToByteArray};
use db_traits::{account::AccountState, base::BaseState};
use lazy_static::lazy_static;
use primitives::{arithmetic::ScalarBig, Address, Balance, Nonce};
use scylla::{Session, _macro_internal::CqlValue};
use std::sync::Arc;
use system::account::{Account, AccountType};
use tokio::sync::Mutex;

// A mutex used to ensure table creation is done atomically.
//
// This mutex is used to synchronize table creation operations to prevent multiple threads from
// concurrently attempting to create the same table in the Cassandra database.
lazy_static! {
	static ref TABLE_CREATION_LOCK: Mutex<()> = Mutex::new(());
}

/// Represents the state of a Cassandra database.
pub struct StateCas {
	/// The Cassandra session used for database operations.
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<Account> for StateCas {
	/// Create the necessary table for account storage if it doesn't exist.
	///
	/// This function is responsible for creating the table used for storing account information
	/// in the Cassandra database if it does not already exist. It ensures atomicity during table
	/// creation by acquiring a lock using the `TABLE_CREATION_LOCK`.
	///
	/// # Returns
	/// - `Ok(())` if the table creation is successful.
	/// - `Err` containing an error if table creation fails.
	async fn create_table(&self) -> Result<(), Error> {
		let _guard = TABLE_CREATION_LOCK.lock().await; // Acquire lock

		// Define the table schema
		let create_query = "CREATE TABLE IF NOT EXISTS l1x.account (
        address blob,
        balance blob,
        nonce blob,
        account_type blob,
        PRIMARY KEY (address)
    );";

		// Execute the query to create the table
		match self.session.query(create_query, &[]).await {
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Create table failed: {:?}", e)),
		}
	}

	/// Insert a new account into the database.
	///
	/// This function is responsible for inserting a new account into the Cassandra database.
	///
	/// # Arguments
	/// - `account`: A reference to the `Account` struct representing the account to be inserted.
	///
	/// # Returns
	/// - `Ok(())` if the insertion is successful.
	/// - `Err` containing an error if the insertion fails.
	async fn create(&self, account: &Account) -> Result<(), Error> {
		// Convert balance and nonce to byte arrays
		let balance_u128: ScalarBig = account.balance.to_byte_array_le(ScalarBig::default());
		let nonce_u128: ScalarBig = account.nonce.to_byte_array_le(ScalarBig::default());

		// Serialize account type to bytes
		let account_type: Vec<u8> = bincode::serialize(&account.account_type)
			.map_err(|_| anyhow!("Unserializable account type - {:?}", account.account_type))?;

		// Define the insert query
		let insert_query =
			"INSERT INTO l1x.account (address, balance, nonce, account_type) VALUES (?, ?, ?, ?)";

		// Execute the query to insert the account
		match self
			.session
			.query(insert_query, (&account.address, &balance_u128, &nonce_u128, &account_type))
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Account: Create account failed - {}", e)),
		}
	}

	/// Update an existing account in the database.
	///
	/// This function updates the balance and nonce of an existing account in the Cassandra
	/// database. If the account does not exist, it creates a new one.
	///
	/// # Arguments
	/// - `account`: A reference to the `Account` struct representing the account to be updated.
	///
	/// # Returns
	/// - `Ok(())` if the update is successful.
	/// - `Err` containing an error if the update fails.
	async fn update(&self, account: &Account) -> Result<(), Error> {
		// Check if the account exists
		if !self.is_valid_account(&account.address).await? {
			// If the account does not exist, create a new one
			self.create(account).await
		} else {
			// Convert balance and nonce to byte arrays
			let balance_u128: ScalarBig = account.balance.to_byte_array_le(ScalarBig::default());
			let nonce_u128: ScalarBig = account.nonce.to_byte_array_le(ScalarBig::default());

			// Define the update query
			let update_query = "UPDATE l1x.account SET balance = ?, nonce = ? WHERE address = ?";

			// Execute the query to update the account
			match self
				.session
				.query(update_query, (&balance_u128, &nonce_u128, &account.address))
				.await
			{
				Ok(_) => Ok(()),
				Err(e) => Err(anyhow!("Account: Update account balance failed - {}", e)),
			}
		}
	}

	/// Execute a raw query on the database.
	///
	/// This function allows executing a custom query directly on the Cassandra database.
	///
	/// # Arguments
	/// - `query`: A string slice containing the raw query to be executed.
	///
	/// # Returns
	/// - `Ok(())` if the query execution is successful.
	/// - `Err` containing an error if the query execution fails.
	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		// Execute the provided raw query
		match self.session.query(query, &[]).await {
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Failed to execute raw query: {}", e)),
		}
	}

	// Set the schema version (not implemented)
	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		todo!()
	}
}

#[async_trait]
impl AccountState for StateCas {
	/// Update the balance of an account in the database.
	///
	/// This function updates the balance of an existing account in the Cassandra database.
	/// If the account does not exist, it creates a new account entry.
	///
	/// # Arguments
	/// - `account`: A reference to the `Account` struct containing the account information.
	///
	/// # Returns
	/// - `Ok(())` if the balance update is successful.
	/// - `Err` containing an error if the balance update fails.
	async fn update_balance(&self, account: &Account) -> Result<(), Error> {
		// Check if the account exists
		if !self.is_valid_account(&account.address).await? {
			// If the account doesn't exist, create a new account entry
			self.create(account).await
		} else {
			// Convert the balance to ScalarBig
			let balance_u128: ScalarBig = account.balance.to_byte_array_le(ScalarBig::default());
			// Define the update query
			let update_query = "UPDATE l1x.account SET balance = ? WHERE address = ?";
			// Execute the query to update the account balance
			match self.session.query(update_query, (&balance_u128, &account.address)).await {
				Ok(_) => Ok(()),
				Err(e) => Err(anyhow!("Account: Update account balance failed - {}", e)),
			}
		}
	}

	/// Increment the nonce of an account in the database.
	///
	/// This function increments the nonce of an existing account in the Cassandra database.
	///
	/// # Arguments
	/// - `address`: A reference to the `Address` of the account whose nonce needs to be
	///   incremented.
	///
	/// # Returns
	/// - `Ok(())` if the nonce increment is successful.
	/// - `Err` containing an error if the nonce increment fails.
	async fn increment_nonce(&self, address: &Address) -> Result<(), Error> {
		// Get the current nonce of the account and increment it by 1
		let nonce = self.get_nonce(address).await? + 1;
		// Convert the nonce to ScalarBig
		let nonce_u128: ScalarBig = nonce.to_byte_array_le(ScalarBig::default());
		// Define the update query
		let update_query = "UPDATE l1x.account SET nonce = ? WHERE address = ?";
		// Execute the query to update the account nonce
		match self.session.query(update_query, (&nonce_u128, &address)).await {
			Ok(_) => Ok(()),
			Err(_) => Err(anyhow!("Invalid address")),
		}
	}

	/// Get an account from the database by its address.
	///
	/// This function retrieves an account from the Cassandra database based on the provided
	/// address. If the account does not exist, it creates a new account with the provided address.
	///
	/// # Arguments
	///
	/// * `address`: A reference to the `Address` of the account to retrieve.
	///
	/// # Returns
	///
	/// * `Result<Account, Error>`: The retrieved account if successful, or an error if the
	///   operation fails.
	///
	/// # Errors
	///
	/// Returns an error if there's a problem with the database query or if the retrieved data
	/// cannot be converted to the expected types.
	async fn get_account(&self, address: &Address) -> Result<Account, Error> {
		// Check if the account exists, create one if it doesn't
		if !self.is_valid_account(address).await? {
			let account = Account::new(*address);
			self.create(&account).await?;
		}

		// Define the select query
		let select_query = "SELECT balance, nonce, account_type FROM l1x.account WHERE address = ?";

		// Execute the query to retrieve the account details
		let query_result = match self.session.query(select_query, (&address,)).await {
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid address")),
		};

		// Extract column indices
		let (balance_idx, _) = query_result
			.get_column_spec("balance")
			.ok_or_else(|| anyhow!("No balance column found"))?;
		let (nonce_idx, _) = query_result
			.get_column_spec("nonce")
			.ok_or_else(|| anyhow!("No nonce column found"))?;
		let (account_type_idx, _) = query_result
			.get_column_spec("account_type")
			.ok_or_else(|| anyhow!("No account_type column found"))?;

		// Extract rows from the query result
		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		// Extract account details from the first row
		let balance: Balance = if let Some(row) = rows.first() {
			if let Some(balance_value) = &row.columns[balance_idx] {
				if let CqlValue::Blob(balance) = balance_value {
					u128::from_byte_array(balance)
				} else {
					return Err(anyhow!("Unable to convert to Balance type"));
				}
			} else {
				return Err(anyhow!("Unable to read balance column"));
			}
		} else {
			return Err(anyhow!("Unable to read row"));
		};

		let nonce: Nonce = if let Some(row) = rows.first() {
			if let Some(nonce_value) = &row.columns[nonce_idx] {
				if let CqlValue::Blob(nonce) = nonce_value {
					u128::from_byte_array(nonce)
				} else {
					return Err(anyhow!("Unable to convert to Nonce type"))
				}
			} else {
				return Err(anyhow!("Unable to read nonce column"))
			}
		} else {
			return Err(anyhow!("account_state: nonce 192 - Unable to read row"))
		};

		let account_type: AccountType = if let Some(row) = rows.first() {
			if let Some(account_type_value) = &row.columns[account_type_idx] {
				if let CqlValue::Blob(account_type) = account_type_value {
					match bincode::deserialize(account_type) {
						Ok(account_type) => account_type,
						Err(e) =>
							return Err(anyhow!("Unable to convert to AccountType type - {}", e)),
					}
				} else {
					return Err(anyhow!("Unable to get bytes for AccountType"))
				}
			} else {
				return Err(anyhow!("Unable to read account_type column"))
			}
		} else {
			return Err(anyhow!("account_state: account_type 211 - Unable to read row"))
		};

		Ok(Account { address: *address, balance, nonce, account_type })
	}

	/// Get the nonce of an account from the database by its address.
	///
	/// This function retrieves the nonce of an account from the Cassandra database based on the
	/// provided address. If the account does not exist, it creates a new account with the provided
	/// address.
	///
	/// # Arguments
	///
	/// * `address`: A reference to the `Address` of the account to retrieve the nonce for.
	///
	/// # Returns
	///
	/// * `Result<Nonce, Error>`: The retrieved nonce if successful, or an error if the operation
	///   fails.
	///
	/// # Errors
	///
	/// Returns an error if there's a problem with the database query or if the retrieved data
	/// cannot be converted to the expected types.
	async fn get_nonce(&self, address: &Address) -> Result<Nonce, Error> {
		// Check if the account exists, create one if it doesn't
		if !self.is_valid_account(address).await? {
			let account = Account::new(*address);
			self.create(&account).await?;
		}

		// Define the select query
		let select_query = "SELECT nonce FROM l1x.account WHERE address = ?";

		// Execute the query to retrieve the nonce
		let query_result = match self.session.query(select_query, (&address,)).await {
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid address")),
		};

		// Extract the nonce column index
		let (nonce_idx, _) = query_result
			.get_column_spec("nonce")
			.ok_or_else(|| anyhow!("No nonce column found"))?;

		// Extract rows from the query result
		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		// Extract the nonce from the first row
		let nonce: Nonce = if let Some(row) = rows.first() {
			if let Some(nonce_value) = &row.columns[nonce_idx] {
				if let CqlValue::Blob(nonce) = nonce_value {
					u128::from_byte_array(nonce)
				} else {
					return Err(anyhow!("Unable to convert to Nonce type"));
				}
			} else {
				return Err(anyhow!("Unable to read nonce column"));
			}
		} else {
			return Err(anyhow!("Unable to read row"));
		};

		Ok(nonce)
	}

	/// Get the balance of an account from the database by its address.
	///
	/// This function retrieves the balance of an account from the Cassandra database based on the
	/// provided address. If the account does not exist, it creates a new account with the provided
	/// address.
	///
	/// # Arguments
	///
	/// * `address`: A reference to the `Address` of the account to retrieve the balance for.
	///
	/// # Returns
	///
	/// * `Result<Balance, Error>`: The retrieved balance if successful, or an error if the
	///   operation fails.
	///
	/// # Errors
	///
	/// Returns an error if there's a problem with the database query or if the retrieved data
	/// cannot be converted to the expected types.
	async fn get_balance(&self, address: &Address) -> Result<Balance, Error> {
		// Check if the account exists, create one if it doesn't
		if !self.is_valid_account(address).await? {
			let account = Account::new(*address);
			self.create(&account).await?;
		}

		// Define the select query
		let select_query = "SELECT balance FROM l1x.account WHERE address = ?";

		// Execute the query to retrieve the balance
		let query_result = match self.session.query(select_query, (&address,)).await {
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid address")),
		};

		// Extract the balance column index
		let (balance_idx, _) = query_result
			.get_column_spec("balance")
			.ok_or_else(|| anyhow!("No balance column found"))?;

		// Extract rows from the query result
		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		// Extract the balance from the first row
		let balance: Balance = if let Some(row) = rows.first() {
			if let Some(balance_value) = &row.columns[balance_idx] {
				if let CqlValue::Blob(balance) = balance_value {
					u128::from_byte_array(balance)
				} else {
					return Err(anyhow!("Unable to convert to Balance type"));
				}
			} else {
				return Err(anyhow!("Unable to read balance column"));
			}
		} else {
			return Err(anyhow!("Unable to read row"));
		};

		Ok(balance)
	}

	/// Check if an account with the given address exists in the database.
	///
	/// This function checks whether an account with the provided address exists in the Cassandra
	/// database.
	///
	/// # Arguments
	///
	/// * `address`: A reference to the `Address` of the account to check for existence.
	///
	/// # Returns
	///
	/// * `Result<bool, Error>`: `true` if the account exists, `false` otherwise, or an error if the
	///   operation fails.
	///
	/// # Errors
	///
	/// Returns an error if there's a problem with the database query or if the retrieved data
	/// cannot be converted to the expected types.
	async fn is_valid_account(&self, address: &Address) -> Result<bool, Error> {
		// Define the select query to count rows with the given address
		let select_query = "SELECT COUNT(*) AS count FROM l1x.account WHERE address = ?";

		// Execute the query to retrieve the count
		let query_result = match self.session.query(select_query, (&address,)).await {
			Ok(q) => q,
			Err(e) => {
				let message = format!("Invalid address - {}", e);
				return Err(anyhow!(message));
			},
		};

		// Extract the count column index
		let (count_idx, _) = query_result
			.get_column_spec("count")
			.ok_or_else(|| anyhow!("No count column found"))?;

		// Extract rows from the query result
		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		// Check if the count is greater than 0
		if let Some(row) = rows.first() {
			if let Some(count_value) = &row.columns[count_idx] {
				if let CqlValue::BigInt(count) = count_value {
					return Ok(count > &0);
				} else {
					return Err(anyhow!("Unable to convert to Nonce type"));
				}
			}
		}
		Ok(false)
	}
}
