#[cfg(test)]
mod tests {
	use crate::{sub_cluster_manager::*, sub_cluster_state::*};
	use anyhow::Error;
	use db::db::{Database, DbTxConn};
	use lazy_static::lazy_static;
	use primitives::*;
	use std::sync::Mutex;
	lazy_static! {
		pub static ref SINGLETON_LOCK: Mutex<()> = Mutex::new(());
	}
	use system::config::Config;

	// Helper function to create a new State for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		println!("database_conn");
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	pub async fn truncate_sub_cluster_table<'a>(sub_cluster_state: &SubClusterState<'a>) {
		sub_cluster_state.raw_query("TRUNCATE sub_cluster;").await;
	}

	#[tokio::test(flavor = "current_thread")]
	async fn test_store_sub_cluster_address_and_load_all_sub_cluster_addresses() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let sub_cluster_state = SubClusterState::new(&db_pool_conn).await.unwrap();
		truncate_sub_cluster_table(&sub_cluster_state).await;

		let sub_cluster_address1: Address = [1; 20];
		let sub_cluster_address2: Address = [2; 20];

		// Store cluster nonces
		sub_cluster_state
			.store_sub_cluster_address(&sub_cluster_address1)
			.await
			.unwrap();
		sub_cluster_state
			.store_sub_cluster_address(&sub_cluster_address2)
			.await
			.unwrap();

		// Load all cluster ids
		let loaded_sub_cluster_addresses =
			sub_cluster_state.load_all_sub_cluster_addresses().await.unwrap().unwrap();

		assert_eq!(loaded_sub_cluster_addresses, vec![sub_cluster_address2, sub_cluster_address1]);

		// drop_table(&sub_cluster_state).await.unwrap();
	}

	#[tokio::test(flavor = "current_thread")]
	async fn test_store_sub_sub_cluster_addresses_and_load_all_sub_cluster_addresses() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let sub_cluster_state = SubClusterState::new(&db_pool_conn).await.unwrap();
		truncate_sub_cluster_table(&sub_cluster_state).await;
		let mut sub_cluster_addresses = vec![[1; 20], [2; 20], [3; 20]];
		sub_cluster_state
			.store_sub_cluster_addresses(&sub_cluster_addresses)
			.await
			.unwrap();
		// Store cluster ids

		// Load all cluster ids
		let mut loaded_sub_cluster_addresses =
			sub_cluster_state.load_all_sub_cluster_addresses().await.unwrap().unwrap();

		assert_eq!(loaded_sub_cluster_addresses.sort(), sub_cluster_addresses.sort());
	}

	#[tokio::test(flavor = "current_thread")]
	async fn test_load_all_sub_cluster_addresses_when_empty() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let sub_cluster_state = SubClusterState::new(&db_pool_conn).await.unwrap();
		truncate_sub_cluster_table(&sub_cluster_state).await;
		// Load all cluster nonces
		let loaded_sub_cluster_addresses =
			sub_cluster_state.load_all_sub_cluster_addresses().await.unwrap();

		assert_eq!(loaded_sub_cluster_addresses, None);
	}

	#[tokio::test(flavor = "current_thread")]
	async fn test_sub_create_cluster() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let sub_cluster_state = SubClusterState::new(&db_pool_conn).await.unwrap();
		truncate_sub_cluster_table(&sub_cluster_state).await;

		let sub_cluster_addresses = vec![[1; 20], [2; 20], [3; 20]];

		// Store cluster ids
		sub_cluster_state
			.store_sub_cluster_addresses(&sub_cluster_addresses.clone())
			.await
			.unwrap();

		// Create a SubClusterManager instance
		let mut sub_cluster_manager = SubClusterManager {};

		// Call the create_sub_cluster method
		sub_cluster_manager
			.create_sub_cluster(&[4; 20], &sub_cluster_state)
			.await
			.unwrap();

		let mut loaded_sub_cluster_addresses =
			sub_cluster_state.load_all_sub_cluster_addresses().await.unwrap().unwrap();
		loaded_sub_cluster_addresses.sort();
		assert_eq!(loaded_sub_cluster_addresses, vec![[1; 20], [2; 20], [3; 20], [4; 20]]);
	}
}
