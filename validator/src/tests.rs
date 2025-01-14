#[cfg(test)]
mod tests {
	use crate::{
		validator_manager::ValidatorManager,
		validator_state::ValidatorState,
	};
	use anyhow::Error;
	use db::db::{Database, DbTxConn};
	use primitives::Address;
	use system::{config::Config, validator::Validator};

	// Helper function to create a new State for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		println!("database_conn");
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	pub async fn truncate_validator_table<'a>(validator_state: &ValidatorState<'a>) {
		validator_state.raw_query("TRUNCATE validator;").await;
	}

	#[tokio::test(flavor = "current_thread")]
	#[serial_test::serial]
	async fn test_store_validator() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let validator_state = ValidatorState::new(&db_pool_conn).await.unwrap();
		truncate_validator_table(&validator_state).await;

		let validator = Validator {
			address: [3; 20],
			cluster_address: [10; 20],
			block_number: 456,
			stake: 5000,
			xscore: 0.8,
		};

		let result = validator_state.store_validator(&validator.clone()).await;

		assert!(result.is_ok());
	}

	#[tokio::test(flavor = "current_thread")]
	#[serial_test::serial]
	async fn test_load_validator() {
		let (db_pool_conn, _config) = database_conn().await.unwrap();
		let validator_state = ValidatorState::new(&db_pool_conn).await.unwrap();
		truncate_validator_table(&validator_state).await;

		let addr = [
			117, 16, 73, 56, 186, 164, 124, 84, 168, 96, 4, 239, 153, 140, 199, 108, 46, 97, 98,
			137,
		];
		let address: Address = Address::from(addr);
		let validator =
			Validator { address, cluster_address: addr, block_number: 456, stake: 5000, xscore: 0.8};

		let result = validator_state.store_validator(&validator.clone()).await;

		assert!(result.is_ok());

		let result = validator_state.load_validator(&address).await;

		assert!(result.is_ok());
		let loaded_validator = result.unwrap();
		assert_eq!(loaded_validator.address, address);
		assert_eq!(loaded_validator.cluster_address, addr);
		assert_eq!(loaded_validator.block_number, 456);
		assert_eq!(loaded_validator.stake, 5000);
		assert_eq!(loaded_validator.xscore, 0.8);
	}

	#[tokio::test(flavor = "current_thread")]
	#[serial_test::serial]
	async fn test_select_validators() {
		// Create sample validators

		let validators = vec![
			Validator {
				address: Address::from([4; 20]),
				cluster_address: [10; 20],
				block_number: 4561,
				stake: 1000,
				xscore: 0.8,
			},
			Validator {
				address: Address::from([5; 20]),
				cluster_address: [10; 20],
				block_number: 4561,
				stake: 500,
				xscore: 0.4,
			},
			Validator {
				address: Address::from([6; 20]),
				cluster_address: [10; 20],
				block_number: 4561,
				stake: 1500,
				xscore: 0.9,
			},
		];

		let (db_pool_conn, _config) = database_conn().await.unwrap();
		let validator_state = ValidatorState::new(&db_pool_conn).await.unwrap();
		truncate_validator_table(&validator_state).await;
		let validator_manager = ValidatorManager {};
		// Select top 2 validators
		let result = validator_manager
			.select_validators(&[1u8; 20], &[1u8; 20], 1, 2, &db_pool_conn)
			.await;

		assert!(result.is_ok());

		/*assert!(validator_state.is_validator(&[6; 20], 4561).await.unwrap());
		assert!(validator_state.is_validator(&[4; 20], 4561).await.unwrap());
		assert!(!validator_state.is_validator(&[5; 20], 4561).await.unwrap());*/
	}
}