#[cfg(test)]
mod tests {
	use cluster::{cluster_manager::ClusterManager, cluster_state::ClusterState};

	use crate::contract_state::*;
	use anyhow::Error;
	use db::db::{Database, DbTxConn};
	use node_info::node_info_state::NodeInfoState;
	use primitives::*;
	use std::convert::TryInto;
	use system::{
		access::AccessType,
		config::Config,
		contract::{Contract, ContractType},
	};

	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	pub async fn truncate_contract_table<'a>(contract_state: &ContractState<'a>) {
		contract_state.raw_query("TRUNCATE contract;").await;
	}
	pub async fn truncate_cluster_address_table<'a>(cluster_state: &ClusterState<'a>) {
		cluster_state.raw_query("TRUNCATE cluster_address;").await;
	}
	pub async fn truncate_node_info_table<'a>(node_info_state: &NodeInfoState<'a>) {
		node_info_state.raw_query("TRUNCATE node_info;").await;
	}
	pub async fn truncate_cluster_table<'a>(cluster_state: &ClusterState<'a>) {
		cluster_state.raw_query("TRUNCATE cluster;").await;
	}

	// Helper function to create a sample contract for testing.
	fn create_sample_contract() -> Contract {
		let contract_address: Address = [0x01; 20].try_into().unwrap();
		let code: ContractCode = vec![0xAA, 0xBB, 0xCC];
		let owner_address: Address = [0xFF; 20].try_into().unwrap();
		Contract {
			address: contract_address,
			code,
			owner_address,
			r#type: ContractType::L1XVM as i8,
			access: AccessType::PUBLIC as i8,
		}
	}

	async fn setup_contract() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		// let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
		let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();
		let node_info_state = NodeInfoState::new(&db_pool_conn).await.unwrap();
		// truncate_contract_table(&contract_state).await;
		truncate_node_info_table(&node_info_state).await;
		truncate_cluster_address_table(&cluster_state).await;
		truncate_cluster_table(&cluster_state).await;

		// Create a ClusterManager instance
		let mut cluster_manager = ClusterManager {};

		// Call the create_cluster method
		let cluster_address = [10u8; 20];
		cluster_manager.create_cluster(&cluster_address, &cluster_state).await.unwrap();
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_contract_and_load_contract() {
		setup_contract().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_state = ContractState::new(&db_pool_conn).await.unwrap();

		let sample_contract = create_sample_contract();
		contract_state
			.store_contract(&sample_contract)
			.await
			.expect("Failed to store contract");
		let loaded_contract = contract_state
			.get_contract(&sample_contract.address)
			.await
			.unwrap_or_else(|err| panic!("Failed to load contract: {}", err));

		assert_eq!(loaded_contract, sample_contract);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_update_contract_code() {
		setup_contract().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_state = ContractState::new(&db_pool_conn).await.unwrap();

		let mut sample_contract = create_sample_contract();
		contract_state
			.store_contract(&sample_contract)
			.await
			.expect("Failed to store contract");
		sample_contract.code = vec![0xDD, 0xEE, 0xFF];
		contract_state
			.update_contract_code(&sample_contract)
			.await
			.expect("Failed to update contract");
		let loaded_contract = contract_state
			.get_contract(&sample_contract.address)
			.await
			.unwrap_or_else(|err| panic!("Failed to load contract: {}", err));

		assert_eq!(loaded_contract, sample_contract);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_contract_owner() {
		setup_contract().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_state = ContractState::new(&db_pool_conn).await.unwrap();

		let sample_contract = create_sample_contract();
		contract_state
			.store_contract(&sample_contract)
			.await
			.expect("Failed to store contract");
		let (access, owner) = contract_state
			.get_contract_owner(&sample_contract.address)
			.await
			.expect("Failed to get contract owner");
		assert_eq!(access, sample_contract.access);
		assert_eq!(owner, sample_contract.owner_address);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_get_all_contracts() {
		setup_contract().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_state = ContractState::new(&db_pool_conn).await.unwrap();

		let sample_contract = create_sample_contract();

		contract_state
			.store_contract(&sample_contract)
			.await
			.expect("Failed to store contract");

		let sample_contract_one = Contract {
			address: [0x02; 20].try_into().unwrap(),
			code: vec![0xDD, 0xEE, 0xFF],
			owner_address: [0xAA; 20].try_into().unwrap(),
			r#type: ContractType::EVM as i8,
			access: AccessType::PUBLIC as i8,
		};
		contract_state
			.store_contract(&sample_contract_one)
			.await
			.expect("Failed to store contract");
		let all_contracts =
			contract_state.get_all_contract().await.expect("Failed to get all contracts");
		assert_eq!(all_contracts.contains(&sample_contract), true);
		assert_eq!(all_contracts.contains(&sample_contract_one), true);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_is_valid_contract() {
		setup_contract().await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let contract_state = ContractState::new(&db_pool_conn).await.unwrap();

		let sample_contract = create_sample_contract();
		contract_state
			.store_contract(&sample_contract)
			.await
			.expect("Failed to store contract");
		let result = contract_state
			.is_valid_contract(&sample_contract.address)
			.await
			.expect("Failed to valid contract");
		assert_eq!(result, true);
	}
}
