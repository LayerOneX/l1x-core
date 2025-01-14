#[cfg(test)]
mod tests {
	use crate::{staking_manager::*, staking_state::*};
	use account::account_state::AccountState;
	use anyhow::Error;
	use db::db::{Database, DbTxConn};
	use primitives::*;
	use system::{
		account::{Account, AccountType},
		config::Config,
		staking_account::StakingAccount,
	};

	// Helper function to create a new State for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		println!("database_conn");
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	async fn perform_table_cleanup() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		let staking_state = StakingState::new(&db_pool_conn).await.unwrap();

		truncate_account_table(&account_state).await;
		truncate_staking_table(&staking_state).await;
	}

	pub async fn truncate_account_table<'a>(account_state: &AccountState<'a>) {
		account_state.raw_query("TRUNCATE account;").await;
	}
	pub async fn truncate_staking_table<'a>(staking_state: &StakingState<'a>) {
		staking_state.raw_query("TRUNCATE staking_account;").await;
		staking_state.raw_query("TRUNCATE staking_pool;").await;
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
	async fn create_new_pool<'a>(
		staking_state: &StakingState<'a>,
		account_state: &AccountState<'a>,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<Address, Error> {
		let staking_manager = StakingManager::new();
		let pool_address: Address = [80u8; 20];
		let pool_owner: Address = [2; 20];
		let contract_instance_address = None;
		let cluster_address: Address = [3; 20];
		let created_block_number = 1;
		let min_stake = Some(1_000);
		let max_stake = None;
		let min_pool_balance = Some(1_000);
		let max_pool_balance = None;
		let staking_period = Some(1_000);
		let res = staking_manager
			.create_pool(
				&pool_address,
				&pool_owner,
				contract_instance_address,
				&cluster_address,
				created_block_number,
				min_stake,
				max_stake,
				min_pool_balance,
				max_pool_balance,
				staking_period,
				&db_pool_conn,
			)
			.await;
		println!("res: {:?}", res);
		Ok(pool_address)
	}

	async fn stake<'a>(
		staking_manager: &StakingManager,
		staking_state: &StakingState<'a>,
		pool_address: &Address,
		account_address: Address,
		amount: Balance,
		account_state: &AccountState<'a>,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let block_number = 1;
		let mut account = create_account(account_address, &account_state).await;
		account.balance = 200000000;
		account_state.update_account(&account).await.unwrap();
		staking_manager
			.stake(&account_address, pool_address, block_number, amount, &db_pool_conn)
			.await?;
		Ok(())
	}

	#[tokio::test(flavor = "current_thread")]
	async fn test_get_all_stakers() {
		perform_table_cleanup().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		let staking_state = StakingState::new(&db_pool_conn).await.unwrap();

		let pool_address =
			create_new_pool(&staking_state, &account_state, &db_pool_conn).await.unwrap();
		println!("pool_address: {:?}", pool_address);
		let staking_manager = StakingManager::new();

		let acc1_stake = 1_000;
		stake(
			&staking_manager,
			&staking_state,
			&pool_address,
			[1u8; 20],
			acc1_stake,
			&account_state,
			&db_pool_conn,
		)
		.await
		.unwrap();

		let acc2_stake = 5_600;
		stake(
			&staking_manager,
			&staking_state,
			&pool_address,
			[2u8; 20],
			acc2_stake,
			&account_state,
			&db_pool_conn,
		)
		.await
		.unwrap();
		let stakers = staking_state.get_all_pool_stakers(&pool_address).await.unwrap();

		// println!("{:?}", stakers);
		let stakers_vec = vec![
			StakingAccount { account_address: [1u8; 20], pool_address, balance: acc1_stake },
			StakingAccount { account_address: [2u8; 20], pool_address, balance: acc2_stake },
		];
		assert_eq!(stakers, stakers_vec);
	}
}
