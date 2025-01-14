#[cfg(test)]
mod tests {
	use crate::contract_instance_state::*;
	use anyhow::Error;
	use db::db::{Database, DbTxConn};
	use primitives::*;
	use std::convert::TryInto;
	use system::{config::Config, contract_instance::ContractInstance};

	// Helper function to create a new State for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	async fn perform_table_cleanup() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
		truncate_contract_table(&contract_instance_state).await;
		truncate_table(&contract_instance_state).await;
		truncate_contract_instance_code_map_table(&contract_instance_state).await;
	}

	pub async fn truncate_table<'a>(contract_instance_state: &ContractInstanceState<'a>) {
		contract_instance_state.raw_query("TRUNCATE contract_instance;").await;
	}
	pub async fn truncate_contract_instance_code_map_table<'a>(
		contract_instance_state: &ContractInstanceState<'a>,
	) {
		contract_instance_state
			.raw_query("TRUNCATE contract_instance_contract_code_map;")
			.await;
	}

	pub async fn truncate_contract_table<'a>(contract_instance_state: &ContractInstanceState<'a>) {
		contract_instance_state.raw_query("TRUNCATE contract;").await;
	}

	// Helper function to create a sample contract_instance for testing.
	fn create_sample_contract_instance() -> ContractInstance {
		let contract_address: Address = [0x01; 20].try_into().unwrap();
		let contract_instance_address: Address = [0x02; 20].try_into().unwrap();
		let code: ContractCode = vec![0xAA, 0xBB, 0xCC];
		let owner_address: Address = [0xFF; 20].try_into().unwrap();
		ContractInstance {
			instance_address: contract_instance_address,
			contract_address,
			owner_address,
		}
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_and_load_contract_instance() {
		perform_table_cleanup().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();

		let sample_contract = create_sample_contract_instance();
		contract_instance_state
			.store_contract_instance(&sample_contract)
			.await
			.expect("Failed to store contract_instance");
		let loaded_contract = contract_instance_state
			.get_contract_instance(&sample_contract.instance_address)
			.await
			.unwrap_or_else(|err| panic!("Failed to load contract_instance: {}", err));

		assert_eq!(loaded_contract, sample_contract);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_update_load_contract_instance_key_value() {
		perform_table_cleanup().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();

		let sample_contract = create_sample_contract_instance();
		contract_instance_state
			.store_contract_instance(&sample_contract)
			.await
			.expect("Failed to store contract_instance");
		let loaded_contract = contract_instance_state
			.get_contract_instance(&sample_contract.instance_address)
			.await
			.unwrap_or_else(|err| panic!("Failed to load contract_instance: {}", err));

		assert_eq!(loaded_contract, sample_contract);
		contract_instance_state
			.store_state_key_value(
				&loaded_contract.instance_address,
				&"key1".as_bytes().to_vec(),
				&"value1".as_bytes().to_vec(),
			)
			.await
			.unwrap();
		let value = match contract_instance_state
			.get_state_key_value(&loaded_contract.instance_address, &"key1".as_bytes().to_vec())
			.await
			.unwrap()
		{
			Some(value) => value,
			None => panic!("Failed to load contract_instance value"),
		};
		assert_eq!(value, "value1".as_bytes().to_vec());
		contract_instance_state
			.update_state_key_value(
				&loaded_contract.instance_address,
				&"key1".as_bytes().to_vec(),
				&"value2".as_bytes().to_vec(),
			)
			.await
			.unwrap();
		let value = match contract_instance_state
			.get_state_key_value(&loaded_contract.instance_address, &"key1".as_bytes().to_vec())
			.await
			.unwrap()
		{
			Some(value) => value,
			None => panic!("Failed to load contract_instance value"),
		};
		assert_eq!(value, "value2".as_bytes().to_vec());
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_is_valid_contract_instance() {
		perform_table_cleanup().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();

		let sample_contract = create_sample_contract_instance();
		contract_instance_state.store_contract_instance(&sample_contract).await.unwrap();
		let valid = contract_instance_state
			.is_valid_contract_instance(&sample_contract.instance_address)
			.await
			.unwrap();
		assert_eq!(valid, true);
	}
}
