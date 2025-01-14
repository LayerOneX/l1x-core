#[cfg(test)]
mod tests {
	use crate::{cluster_manager::*, cluster_state::*};
	use anyhow::{anyhow, Error};
	use db::db::{Database, DbTxConn};
	use primitives::*;
	use system::config::Config;

	// Helper function to create a new AccountState for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	pub async fn truncate_table<'a>(cluster_state: &ClusterState<'a>) -> Result<(), Error> {
		match cluster_state.raw_query("TRUNCATE cluster;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("TRUNCATE table failed")),
		};
		Ok(())
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_cluster_address_and_load_all_cluster_addresses() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();
		truncate_table(&cluster_state).await.unwrap();

		let cluster_address1: Address = [1; 20];
		let cluster_address2: Address = [2; 20];

		// Store cluster nonces
		// println!("{:?}", cluster_state.store_cluster_address(&cluster_address1).await);
		cluster_state.store_cluster_address(&cluster_address1).await.unwrap();
		cluster_state.store_cluster_address(&cluster_address2).await.unwrap();

		// Load all cluster ids
		let loaded_cluster_addresses =
			cluster_state.load_all_cluster_addresses().await.unwrap().unwrap();

		assert_eq!(loaded_cluster_addresses, vec![cluster_address2, cluster_address1]);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_cluster_addresses_and_load_all_cluster_addresses() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();
		truncate_table(&cluster_state).await.unwrap();
		let mut cluster_addresses = vec![[1; 20], [2; 20], [3; 20]];

		// Store cluster ids
		cluster_state.store_cluster_addresses(&cluster_addresses.clone()).await.unwrap();

		// Load all cluster ids
		let mut loaded_cluster_addresses =
			cluster_state.load_all_cluster_addresses().await.unwrap().unwrap();

		assert_eq!(loaded_cluster_addresses.sort(), cluster_addresses.sort());
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_load_all_cluster_addresses_when_empty() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();
		truncate_table(&cluster_state).await.unwrap();
		// Load all cluster nonces
		let loaded_cluster_addresses = cluster_state.load_all_cluster_addresses().await.unwrap();

		assert!(loaded_cluster_addresses.is_none());
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_create_cluster() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();
		truncate_table(&cluster_state).await.unwrap();
		let cluster_addresses = vec![[1; 20], [2; 20], [3; 20]];
		// Store cluster ids
		cluster_state.store_cluster_addresses(&cluster_addresses.clone()).await.unwrap();
		// Create a ClusterManager instance
		let mut cluster_manager = ClusterManager {};

		// Call the create_cluster method
		cluster_manager.create_cluster(&[4; 20], &cluster_state).await.unwrap();

		let mut loaded_cluster_addresses =
			cluster_state.load_all_cluster_addresses().await.unwrap().unwrap();
		loaded_cluster_addresses.sort();
		assert_eq!(loaded_cluster_addresses, vec![[1; 20], [2; 20], [3; 20], [4; 20]]);
	}
}
