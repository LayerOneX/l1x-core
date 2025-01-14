extern crate anyhow;
use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use bigdecimal::ToPrimitive;
use db::postgres::{
	pg_models::{self, AccountType, QueryAccount},
	postgres::{PgConnectionType, PostgresDBConn},
	schema::account::dsl as account_dsl,
};
use db_traits::{account::AccountState, base::BaseState};
use diesel::{self, prelude::*};
use primitives::{Address, Balance, Nonce};
use system::account::Account;
use util::convert::convert_to_big_decimal_balance;

// Define a struct to represent the state of accounts in a PostgreSQL database.
#[derive(Clone)]
pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}
// Implement BaseState trait for Account operations asynchronously.
#[async_trait]
impl<'a> BaseState<Account> for StatePg<'a> {
	// Create table not needed for postgres, the tables are created using migration scripts
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}
	/// Asynchronously creates a new account in the PostgreSQL database.
	///
	/// # Arguments
	///
	/// * `acc` - A reference to the account to be created.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - An empty result indicating success or an error if the creation
	async fn create(&self, acc: &Account) -> Result<(), Error> {
		// Import the DSL (Domain Specific Language) for the account schema
		use db::postgres::schema::account::dsl::*;

		// Convert balance and nonce to U128
		let balance_u128 = convert_to_big_decimal_balance(acc.balance);
		let nonce_u128 = convert_to_big_decimal_balance(acc.nonce);

		// Extract account type string and convert it to an enum
		let account_type_str = acc.account_type.as_str();
		let account_type_enum: AccountType = match account_type_str {
			"System" => AccountType::System,
			"User" => AccountType::User,
			_ => return Err(anyhow!("Invalid account type: {}", account_type_str)),
		};

		// Create a new account object with the provided data
		let new_account = pg_models::NewAccount {
			address: hex::encode(acc.address),
			account_type: Some(account_type_enum),
			balance: Some(balance_u128),
			nonce: Some(nonce_u128),
		};

		// Insert the new account into the database
		match &self.pg.conn {
			// Execute the insert operation in a transactional connection
			PgConnectionType::TxConn(conn) => {
				match diesel::insert_into(account)
					.values(new_account)
					.on_conflict(address)
					.do_nothing()
					.execute(*conn.lock().await)
				{
					Ok(_) => Ok(()),
					Err(e) => Err(anyhow::anyhow!("Failed to insert new account: {}", e)),
				}
			},
			// Execute the insert operation in a regular connection
			PgConnectionType::PgConn(conn) => {
				match diesel::insert_into(account)
					.values(new_account)
					.on_conflict(address)
					.do_nothing()
					.execute(&mut *conn.lock().await)
				{
					Ok(_) => Ok(()), // Return Ok(()) if insertion is successful
					Err(e) => Err(anyhow::anyhow!("Failed to insert new account: {}", e)), /* Return an error if insertion fails */
				}
			},
		}
	}

	/// Asynchronously updates an existing account in the PostgreSQL database.
	///
	/// If the account with the given address does not exist, it creates a new account
	/// with the provided account details.
	///
	/// # Arguments
	///
	/// * `acc` - A reference to the account with updated information.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - An empty result indicating success or an error if the update fails.
	async fn update(&self, acc: &Account) -> Result<(), Error> {
		// Import the DSL (Domain Specific Language) for the account schema
		use db::postgres::schema::account::dsl::*;

		// Check if the account with the given address exists
		if !self.is_valid_account(&acc.address).await? {
			// If the account does not exist, create a new one with the provided details
			self.create(acc).await?
		} else {
			// If the account exists, extract new balance and nonce
			let new_balance = convert_to_big_decimal_balance(acc.balance);
			let new_nonce = convert_to_big_decimal_balance(acc.nonce);
			let addr = hex::encode(acc.address);

			// Update the existing account with new balance and nonce values
			match &self.pg.conn {
				// Execute the update operation in a transactional connection
				PgConnectionType::TxConn(conn) => {
					diesel::update(account.filter(address.eq(addr)))
						.set((balance.eq(new_balance), nonce.eq(new_nonce))) // set new values for balance and nonce
						.execute(*conn.lock().await)?; // Execute the update query and wait for completion
				},
				// Execute the update operation in a regular connection
				PgConnectionType::PgConn(conn) => {
					diesel::update(account.filter(address.eq(addr)))
						.set((balance.eq(new_balance), nonce.eq(new_nonce))) // set new values for balance and nonce
						.execute(&mut *conn.lock().await)?; // Execute the update query and wait for completion
				},
			}
		}
		Ok(()) // Return Ok(()) indicating success
	}

	/// Asynchronously executes a raw SQL query on the PostgreSQL database.
	///
	/// This function allows executing arbitrary SQL queries directly on the database
	/// without using Diesel's query builder. It's primarily used for executing complex
	/// or specialized SQL commands.
	///
	/// # Arguments
	///
	/// * `query` - A string containing the raw SQL query to execute.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - An empty result indicating success or an error if the query
	///   execution fails.
	async fn raw_query(&self, query: &str) -> std::result::Result<(), Error> {
		match &self.pg.conn {
			// Execute the raw SQL query in a transactional connection
			PgConnectionType::TxConn(conn) => {
				diesel::sql_query(query).execute(*conn.lock().await)?; // Execute the query and wait for completion
			},
			// Execute the raw SQL query in a regular connection
			PgConnectionType::PgConn(conn) => {
				diesel::sql_query(query).execute(&mut *conn.lock().await)?; // Execute the query and wait for
				                                            // completion
			},
		}
		Ok(()) // Return Ok(()) indicating success
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		todo!()
	}
}

#[async_trait]
impl<'a> AccountState for StatePg<'a> {
	async fn update_balance(&self, acc: &Account) -> std::result::Result<(), Error> {
		use db::postgres::schema::account::dsl::*;

		if !self.is_valid_account(&acc.address).await? {
			self.create(acc).await?
		} else {
			let new_balance = convert_to_big_decimal_balance(acc.balance);
			let addr = hex::encode(acc.address);
			let updated_rows = match &self.pg.conn {
				PgConnectionType::TxConn(conn) => diesel::update(account.filter(address.eq(addr)))
					.set(balance.eq(new_balance))
					.execute(*conn.lock().await),
				PgConnectionType::PgConn(conn) => diesel::update(account.filter(address.eq(addr)))
					.set(balance.eq(new_balance))
					.execute(&mut *conn.lock().await),
			}?;

			if updated_rows != 1 {
				return Err(anyhow!(
					"Account update failed: expected to update balance, updated {}",
					updated_rows
				));
			}
		}
		Ok(())
	}

	/// Asynchronously increments the nonce of the account associated with the given address.
	///
	/// This function retrieves the current nonce of the account, increments it by one,
	/// and updates the database with the new nonce value.
	///
	/// # Arguments
	///
	/// * `addr` - A reference to the address of the account whose nonce needs to be incremented.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - An empty result indicating success or an error if the operation
	///   fails.
	async fn increment_nonce(&self, addr: &Address) -> Result<(), Error> {
		use db::postgres::schema::account::dsl::*;

		// Retrieve the current nonce for the account and increment it by one
		let result_nonce = self.get_nonce(addr).await? + 1;
		let new_nonce = convert_to_big_decimal_balance(result_nonce);
		let encoded_address = hex::encode(addr);

		// Update the database with the new nonce value
		match &self.pg.conn {
			// Execute the update query in a transactional connection
			PgConnectionType::TxConn(conn) => {
				diesel::update(account.filter(address.eq(encoded_address)))
					.set(nonce.eq(new_nonce))
					.execute(*conn.lock().await)?; // Execute the query and wait for completion
			},
			// Execute the update query in a regular connection
			PgConnectionType::PgConn(conn) => {
				diesel::update(account.filter(address.eq(encoded_address)))
					.set(nonce.eq(new_nonce))
					.execute(&mut *conn.lock().await)?; // Execute the query and wait for completion
			},
		}
		Ok(()) // Return Ok(()) indicating success
	}

	/// Asynchronously retrieves an account from the database based on the provided address.
	///
	/// This function fetches the account details associated with the given address from the
	/// database.
	///
	/// # Arguments
	///
	/// * `addr` - A reference to the address of the account to retrieve.
	///
	/// # Returns
	///
	/// * `Result<Account, Error>` - The retrieved account if it exists, or an error if the account
	///   does not exist or if there's an issue with the database operation.
	async fn get_account(&self, addr: &Address) -> Result<Account, Error> {
		use db::postgres::schema::account::dsl::*;

		// Encode the address into a hexadecimal string for database query
		let encoded_address = hex::encode(addr);

		// Execute the database query to fetch the account details based on the address
		let res = match &self.pg.conn {
			// Execute the query in a transactional connection
			PgConnectionType::TxConn(conn) => account
				.filter(account_dsl::address.eq(encoded_address))
				.load::<QueryAccount>(*conn.lock().await),
			// Execute the query in a regular connection
			PgConnectionType::PgConn(conn) => account
				.filter(account_dsl::address.eq(encoded_address))
				.load::<QueryAccount>(&mut *conn.lock().await),
		}?;

		// Check if the query result contains any account details
		match res.first() {
			Some(response) => {
				// Extract account type from the query response
				let acc_type = match response.account_type.clone() {
					Some(raw_acc_type) => system::account::AccountType::from(raw_acc_type.clone()),
					None => {
						anyhow::bail!("acc_type does not exist")
					},
				};

				// Decode the hexadecimal address back to a byte array
				let mut address_bytes = [0u8; 20];
				match hex::decode(&response.address) {
					Ok(decoded) => {
						address_bytes.copy_from_slice(&decoded);
						println!("Decoded bytes: {:?}", hex::encode(address_bytes));
					},
					Err(e) => {
						println!("Failed to decode hex string: {:?}", e);
					},
				}

				// Construct an Account struct using the retrieved details
				let acc = Account {
					address: address_bytes,
					balance: response.balance.as_ref().unwrap().to_u128().unwrap(),
					nonce: response.nonce.as_ref().unwrap().to_u128().unwrap(),
					account_type: acc_type,
				};

				Ok(acc) // Return the retrieved account
			},
			None => {
				anyhow::bail!("account does not exist") // Return an error if the account doesn't exist
			},
		}
	}

	/// Asynchronously retrieves the nonce associated with the given account address.
	///
	/// This function fetches the nonce (transaction count) associated with the provided account
	/// address from the database.
	///
	/// # Arguments
	///
	/// * `addr` - A reference to the address of the account for which to retrieve the nonce.
	///
	/// # Returns
	///
	/// * `Result<Nonce, Error>` - The nonce value if the account exists, or an error if the account
	///   does not exist or if there's an issue with the database operation.
	async fn get_nonce(&self, addr: &Address) -> Result<Nonce, Error> {
		use db::postgres::schema::account::dsl::*;

		// Check if the provided address corresponds to a valid account
		if !self.is_valid_account(addr).await? {
			// If the account does not exist, return a default nonce value of 0
			Ok(0)
		} else {
			// Encode the address into a hexadecimal string for database query
			let add = hex::encode(addr);

			// Execute the database query to fetch the account details based on the address
			let res = match &self.pg.conn {
				// Execute the query in a transactional connection
				PgConnectionType::TxConn(conn) =>
					account.filter(address.eq(add)).load::<QueryAccount>(*conn.lock().await),
				// Execute the query in a regular connection
				PgConnectionType::PgConn(conn) =>
					account.filter(address.eq(add)).load::<QueryAccount>(&mut *conn.lock().await),
			}?;

			// Check if the query result contains any account details
			match res.first() {
				Some(acc) => match &acc.nonce {
					Some(big_decimal_nonce) => {
						// Convert the nonce from BigDecimal to u128
						let nonce_u128: u128 = big_decimal_nonce.to_u128().ok_or_else(|| {
							anyhow!("Failed to convert BigDecimal to u128 for nonce")
						})?;
						Ok(nonce_u128) // Return the retrieved nonce value
					},
					None => Err(anyhow!("Nonce is None")), // Return an error if nonce is not found
				},
				None => Err(anyhow!("Account not found")), /* Return an error if the account is
				                                            * not found */
			}
		}
	}

	/// Asynchronously retrieves the balance associated with the given account address.
	///
	/// This function fetches the balance associated with the provided account address from the
	/// database.
	///
	/// # Arguments
	///
	/// * `addr` - A reference to the address of the account for which to retrieve the balance.
	///
	/// # Returns
	///
	/// * `Result<Balance, Error>` - The balance value if the account exists, or an error if the
	///   account does not exist or if there's an issue with the database operation.
	async fn get_balance(&self, addr: &Address) -> Result<Balance, Error> {
		use db::postgres::schema::account::dsl::*;

		// Encode the address into a hexadecimal string for database query
		let add = hex::encode(addr);

		// Execute the database query to fetch the account details based on the address
		let res = match &self.pg.conn {
			// Execute the query in a transactional connection
			PgConnectionType::TxConn(conn) =>
				account.filter(address.eq(add)).load::<QueryAccount>(*conn.lock().await),
			// Execute the query in a regular connection
			PgConnectionType::PgConn(conn) =>
				account.filter(address.eq(add)).load::<QueryAccount>(&mut *conn.lock().await),
		}?;

		// Check if the query result contains any account details
		match res.first() {
			Some(acc) => match &acc.balance {
				Some(big_decimal_balance) => {
					// Convert the balance from BigDecimal to u128
					let balance_u128: u128 = big_decimal_balance.to_u128().ok_or_else(|| {
						anyhow!("Failed to convert BigDecimal to u128 for balance")
					})?;
					Ok(balance_u128) // Return the retrieved balance value
				},
				None => Err(anyhow!("Balance is None")), // Return an error if balance is not found
			},
			None => Err(anyhow!("Account not found")), /* Return an error if the account is not
			                                            * found */
		}
	}

	/// Asynchronously checks if an account with the given address exists in the database.
	///
	/// This function verifies whether an account with the provided address exists in the database.
	///
	/// # Arguments
	///
	/// * `addr` - A reference to the address of the account to check for existence.
	///
	/// # Returns
	///
	/// * `anyhow::Result<bool>` - `true` if the account exists, `false` if it doesn't exist, or an
	///   error if there's an issue with the database operation.
	async fn is_valid_account(&self, addr: &Address) -> anyhow::Result<bool> {
		use db::postgres::schema::account::dsl::*;

		// Encode the address into a hexadecimal string for database query
		let add = hex::encode(addr);

		// Execute the database query to check if an account with the given address exists
		let query_result = match &self.pg.conn {
			// Execute the query in a transactional connection
			PgConnectionType::TxConn(conn) =>
				account.filter(address.eq(add)).load::<QueryAccount>(*conn.lock().await),
			// Execute the query in a regular connection
			PgConnectionType::PgConn(conn) =>
				account.filter(address.eq(add)).load::<QueryAccount>(&mut *conn.lock().await),
		};

		// Check the result of the query execution
		match query_result {
			Ok(res) => Ok(!res.is_empty()), /* Return true if the result set is not empty */
			// (account exists), otherwise false
			Err(e) => Err(anyhow::anyhow!("Failed to execute query: {:?}", e)), /* Return an error if the query execution fails */
		}
	}
}
