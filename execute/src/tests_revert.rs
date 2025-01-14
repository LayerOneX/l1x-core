#[cfg(test)]
mod tests {
	use account::account_state::AccountState;
	use anyhow::Error;
	use db::db::{Database, DbTxConn};
	use primitives::Address;
	use serial_test::serial;
	use system::{
		account::{Account, AccountType},
		config::Config,
	};

	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	async fn create_account<'a>(address: Address, account_state: &AccountState<'a>) -> Account {
		let account = Account::new(address);
		account_state.create_account(&account).await.unwrap();
		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
		assert_eq!(loaded_account.account_type, AccountType::User);
		account
	}

	pub async fn truncate_table<'a>(account_state: &AccountState<'a>) -> Result<(), Error> {
		let delete_query = "TRUNCATE account;";
		match account_state.raw_query(delete_query).await {
			Ok(_) => Ok(()),
			Err(e) => Err(e.into()), // Convert the error to the appropriate type
		}
	}
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
}
