#[cfg(test)]
mod tests {
	use crate::{account_manager::*, account_state::*};
	use anyhow::Error;
	use db::db::{Database, DbTxConn};
	use primitives::{Address, Balance};
	use serial_test::serial;
	use system::{
		account::{Account, AccountType},
		config::Config,
	};

	/// Asynchronously establishes a connection to the database and returns the connection along
	/// with the database configuration.
	///
	/// This function initializes a database connection using the provided configuration data,
	/// retrieves a connection from the connection pool, and returns both the database connection
	/// and the configuration.
	///
	/// # Returns
	///
	/// * `Result<(DbTxConn<'a>, Config), Error>` - A tuple containing the database connection and
	///   the configuration if the connection is successfully established, or an error if the
	///   operation fails.
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	/// Asynchronously creates a new account in the database.
	///
	/// This function creates a new account with the provided address, inserts it into the database
	/// using the provided account state, and verifies that the account creation was successful by
	/// retrieving the account from the database.
	///
	/// # Arguments
	///
	/// * `address` - The address for the new account.
	/// * `account_state` - A reference to the account state used to interact with the database.
	///
	/// # Returns
	///
	/// * `Account` - The newly created account.
	///
	/// # Panics
	///
	/// This function panics if there is an error during account creation or if the retrieved
	/// account from the database does not match the expected values.
	async fn create_account<'a>(address: Address, account_state: &AccountState<'a>) -> Account {
		let account = Account::new(address);
		account_state.create_account(&account).await.unwrap();
		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
		assert_eq!(loaded_account.account_type, AccountType::User);
		account
	}

	/// Asynchronously truncates the `account` table in the database.
	///
	/// This function executes a SQL query to truncate the `account` table, removing all its
	/// contents. It utilizes the provided `AccountState` to execute the raw SQL query.
	///
	/// # Arguments
	///
	/// * `account_state` - A reference to the `AccountState` used to interact with the database.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - An empty result indicating success or an error if the operation
	///   fails.
	pub async fn truncate_table<'a>(account_state: &AccountState<'a>) -> Result<(), Error> {
		let delete_query = "TRUNCATE account;";
		match account_state.raw_query(delete_query).await {
			Ok(_) => Ok(()),
			Err(e) => Err(e.into()), // Convert the error to the appropriate type
		}
	}

	/// Tests the creation of an account.
	///
	/// This test function verifies that the creation of a new account works as expected by setting
	/// up a database connection, truncating the `account` table, creating a new account, and
	/// asserting the correctness of the created account's attributes.
	#[tokio::test]
	#[serial]
	async fn test_create_account() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [1; 20];
		create_account(address, &account_state).await;
	}

	/// Tests the update operation for an account.
	///
	/// This test function verifies that updating an existing account in the database works
	/// correctly. It sets up a database connection, truncates the `account` table, creates a new
	/// account, updates its balance and nonce, performs the update operation, and finally asserts
	/// the correctness of the updated account's attributes.
	#[tokio::test]
	#[serial]
	async fn test_update_account() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [2; 20];
		let mut account = create_account(address, &account_state).await;
		account.balance = 200;
		account.nonce = 10;

		account_state.update_account(&account).await.unwrap();
		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
		assert_eq!(loaded_account.account_type, AccountType::User);
	}

	/// Tests the update operation for an account when the account doesn't exist.
	///
	/// This test function verifies that updating an account that doesn't exist in the database will
	/// create a new account with the provided details. It sets up a database connection, truncates
	/// the `account` table, attempts to update a non-existent account, performs the update
	/// operation, and finally asserts the correctness of the newly created account's attributes.
	#[tokio::test]
	#[serial]
	async fn test_update_account_creates_new_account() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [16; 20];
		let mut account = Account::new(address);
		account.balance = 200;
		account.nonce = 10;

		account_state.update_account(&account).await.unwrap();
		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
		assert_eq!(loaded_account.account_type, AccountType::User);
	}

	/// Tests the method to update an existing system account or create a new one if it doesn't
	/// exist.
	///
	/// This test function verifies the correctness of the `update_account` method of the
	/// `AccountState` struct when creating a new system account.
	#[tokio::test]
	#[serial]
	async fn test_update_account_creates_new_system_account() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [17; 20];
		let mut account = Account::new_system(address);
		account.balance = 200;
		account.nonce = 10;

		account_state.update_account(&account).await.unwrap();
		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
		assert_eq!(loaded_account.account_type, AccountType::System);
	}

	/// Tests the update of an account's balance.
	///
	/// This test function verifies that the balance of an existing account can be updated
	/// successfully. It sets up a database connection, truncates the `account` table, creates a new
	/// account, updates its balance, performs the update operation, and finally asserts the
	/// correctness of the updated balance and nonce.
	#[tokio::test]
	#[serial]
	async fn test_update_balance() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [3; 20];
		let mut account = create_account(address, &account_state).await;
		account.balance = 100;
		account.nonce = 10;

		account_state.update_balance(&account).await.unwrap();

		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, 0);
	}

	/// Tests the increment of an account's nonce.
	///
	/// This test function verifies that the nonce of an existing account can be incremented
	/// successfully. It sets up a database connection, truncates the `account` table, creates a new
	/// account, increments its nonce, performs the increment operation, and finally asserts the
	/// correctness of the updated balance and nonce.
	#[tokio::test]
	#[serial]
	async fn test_increment_nonce() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [4; 20];
		let mut account = create_account(address, &account_state).await;
		account.balance = 100;

		account_state.increment_nonce(&address).await.unwrap();

		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, 0);
		assert_eq!(loaded_account.nonce, account.nonce + 1);
	}

	/// Tests the retrieval of an account from the database.
	///
	/// This test function verifies that an account can be retrieved successfully from the database.
	/// It sets up a database connection, truncates the `account` table, creates a new account,
	/// updates its balance and nonce, performs the account retrieval operation, and finally asserts
	/// the correctness of the retrieved account's balance and nonce.
	#[tokio::test]
	#[serial]
	async fn test_get_account() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [5; 20];
		let mut account = create_account(address, &account_state).await;
		account.balance = 100;
		account.nonce = 5;

		account_state.update_account(&account).await.unwrap();

		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
	}

	/// Tests the retrieval of the nonce associated with an account.
	///
	/// This test function verifies that the nonce associated with an account can be retrieved
	/// successfully from the database. It sets up a database connection, truncates the `account`
	/// table, creates a new account, updates its balance and nonce, performs the nonce retrieval
	/// operation, and finally asserts the correctness of the retrieved nonce.
	#[tokio::test]
	#[serial]
	async fn test_get_nonce() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [6; 20];
		let mut account = create_account(address, &account_state).await;
		account.balance = 0;
		account.nonce = 5;

		account_state.update_account(&account).await.unwrap();

		let nonce = account_state.get_nonce(&address).await.unwrap();
		assert_eq!(nonce, account.nonce);
	}

	/// Tests the validity check for an account in the database.
	///
	/// This test function verifies that the validity of an account in the database can be checked
	/// successfully. It sets up a database connection, truncates the `account` table, creates a new
	/// account, and then checks whether the account exists in the database. It asserts that the
	/// account is valid.
	#[tokio::test]
	#[serial]
	async fn test_is_valid_account() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [7; 20];
		let _account = create_account(address, &account_state).await;
		let is_valid_account = account_state.is_valid_account(&address).await.unwrap();
		assert!(account_state.is_valid_account(&address).await.is_ok());
		assert!(is_valid_account);
	}

	/// Tests the creation of a new account.
	///
	/// This test function verifies that a new account can be created successfully. It sets up a
	/// database connection, truncates the `account` table, creates a new account, and then
	/// retrieves the created account from the database. It asserts that the retrieved account
	/// matches the expected account details.
	#[tokio::test]
	#[serial]
	async fn test_new_account() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [8; 20];
		let account_manager = AccountManager::new(&address, &account_state).await.unwrap();

		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account_manager.account.balance);
		assert_eq!(loaded_account.nonce, account_manager.account.nonce);
	}

	/// Tests the retrieval of the balance for an account.
	///
	/// This test function verifies that the balance of an account can be retrieved successfully. It
	/// sets up a database connection, truncates the `account` table, creates a new account, and
	/// then retrieves the balance of the created account using the `get_balance` method of the
	/// `AccountManager`. It asserts that the retrieved balance matches the expected balance of the
	/// account.
	#[tokio::test]
	#[serial]
	async fn test_get_balance() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [9; 20];
		let account_manager = AccountManager::new(&address, &account_state).await.unwrap();

		let balance = account_manager.get_balance();
		assert_eq!(balance, account_manager.account.balance);
	}

	/// Tests the retrieval of the current nonce for an account.
	///
	/// This test function verifies that the current nonce of an account can be retrieved
	/// successfully. It sets up a database connection, truncates the `account` table, creates a new
	/// account, and then retrieves the current nonce of the created account using the
	/// `get_current_nonce` method of the `AccountManager`. It asserts that the retrieved nonce
	/// matches the initial nonce of the account and that the account's nonce is updated after the
	/// retrieval.
	#[tokio::test]
	#[serial]
	async fn test_get_current_nonce() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [10; 20];
		let mut account_manager = AccountManager::new(&address, &account_state).await.unwrap();

		let initial_nonce = account_manager.account.nonce;
		let next_nonce = account_manager.get_current_nonce(&account_state).await.unwrap();

		assert_eq!(next_nonce, initial_nonce);

		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.nonce, next_nonce);
	}

	/// Tests the transfer of funds from one account to another when the sender has sufficient
	/// balance.
	///
	/// This test function verifies that funds can be transferred successfully from one account to
	/// another when the sender has a sufficient balance. It sets up two account managers
	/// representing the sender and receiver accounts, initializes their balances, performs a
	/// transfer operation, and verifies that the balances are updated correctly after the transfer.
	#[tokio::test]
	#[serial]
	async fn test_transfer_sufficient_balance() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address1: Address = [11; 20];
		let mut account_manager1 = AccountManager::new(&address1, &account_state).await.unwrap();

		let address2: Address = [12; 20];
		let account_manager2 = AccountManager::new(&address2, &account_state).await.unwrap();

		let transfer_amount = 50;
		account_manager1.account.balance = 500;
		account_state.update_balance(&account_manager1.account).await.unwrap();
		account_manager1
			.transfer(&address2, &transfer_amount, &account_state)
			.await
			.unwrap();

		let loaded_account1 = account_state.get_account(&address1).await.unwrap();
		let loaded_account2 = account_state.get_account(&address2).await.unwrap();

		assert_eq!(loaded_account1.balance, account_manager1.account.balance);
		assert_eq!(loaded_account2.balance, account_manager2.account.balance + transfer_amount);
	}

	/// Tests the transfer of funds from one account to another when the sender has insufficient
	/// balance.
	///
	/// This test function verifies that an error is returned when attempting to transfer funds from
	/// one account to another where the sender does not have a sufficient balance. It sets up two
	/// account managers representing the sender and receiver accounts, initializes their balances,
	/// attempts a transfer operation with an amount exceeding the sender's balance, and verifies
	/// that the expected error is returned and that the account balances remain unchanged.
	#[tokio::test]
	#[serial]
	async fn test_transfer_insufficient_balance() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address1: Address = [13; 20];
		let mut account_manager1 = AccountManager::new(&address1, &account_state).await.unwrap();

		let address2: Address = [14; 20];
		let account_manager2 = AccountManager::new(&address2, &account_state).await.unwrap();

		let transfer_amount = 150;

		let result = account_manager1.transfer(&address2, &transfer_amount, &account_state).await;

		assert!(result.is_err());
		assert_eq!(result.unwrap_err().to_string(), "Insufficient balance");

		let loaded_account1 = account_state.get_account(&address1).await.unwrap();
		let loaded_account2 = account_state.get_account(&address2).await.unwrap();

		assert_eq!(loaded_account1.balance, account_manager1.account.balance);
		assert_eq!(loaded_account2.balance, account_manager2.account.balance);
	}

	/// Tests the method to check if an account has a sufficient balance.
	///
	/// This test function verifies the correctness of the `has_sufficient_balance` method of the
	/// `AccountManager` struct. It creates an `AccountManager` instance, sets up its balance, and
	/// checks whether the method returns the expected results for different balance values.
	#[tokio::test]
	#[serial]
	async fn test_has_sufficient_balance() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_table(&account_state).await.unwrap();

		let address: Address = [15; 20];
		let account_manager = AccountManager::new(&address, &account_state).await.unwrap();

		assert!(account_manager.has_sufficient_balance(&0));
		assert!(!account_manager.has_sufficient_balance(&50));
		assert!(!account_manager.has_sufficient_balance(&150));
	}

	/// Test the `mint` function of the `AccountManager` struct.
	#[tokio::test]
	async fn test_mint() {
		// Establish a connection to the database and initialize the account state
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();

		// Clear the account state by truncating the table
		truncate_table(&account_state).await.unwrap();

		// Define a sample address
		let address: Address = [16; 20];

		// Create an AccountManager instance with the sample address and account state
		let mut account_manager = AccountManager::new(&address, &account_state).await.unwrap();

		// Act: Mint 50 units
		let result = account_manager.mint(50, &account_state).await;

		// Assert: Minting should be successful
		assert!(result.is_ok());

		// Get the account from the account state and check if the balance is updated correctly
		let account = account_state.get_account(&address).await.unwrap();
		assert_eq!(account.balance, 50); // Balance of the account should be increased by 50

		// Act: Mint additional 100 units
		let result = account_manager.mint(100, &account_state).await;

		// Assert: Minting should be successful
		assert!(result.is_ok());

		// Get the account from the account state again and check if the balance is updated
		// correctly
		let account = account_state.get_account(&address).await.unwrap();
		assert_eq!(account.balance, 150); // Balance in the account state should be updated
	}
}
