
#[cfg(test)]
mod tests {
	use anyhow::Error;
	use db::db::{Database, DbTxConn};
	use crate::node_health_manager::NodeHealthManager;
	use system::node_health::NodeHealth;
	use crate::state_pg::StatePg;
	use crate::state_cas::StateCas;
	use crate::state_rocks::StateRock;
	use tokio;
	use db_traits::node_health::NodeHealthState as NodeHealthState;

	async fn create_test_node_health() -> NodeHealth {
		NodeHealth {
			measured_peer_id: "test_measured_peer".to_string(),
			peer_id: "test_peer".to_string(),
			epoch: 1,
			joined_epoch: 1,
			uptime_percentage: 99.9,
			response_time_ms: 100,
			transaction_count: 1000,
			block_proposal_count: 50,
			anomaly_score: 0.1,
			node_health_version: 1,
		}
	}

	async fn test_node_health_state<T: NodeHealthState>(state: &T) -> Result<(), Error> {
		let node_health = create_test_node_health().await;

		// Test storing node health
		state.store_node_health(&node_health).await?;

		// Test loading node health
		let loaded_health = state.load_node_health(&node_health.peer_id, node_health.epoch).await?;
		assert!(loaded_health.is_some(), "Failed to load stored node health");
		let loaded_health = loaded_health.unwrap();
		assert_eq!(loaded_health, node_health, "Loaded node health does not match stored node health");

		// Test updating node health
		let mut updated_health = node_health.clone();
		updated_health.uptime_percentage = 99.8;
		updated_health.transaction_count = 1500;
		state.update_node_health(&updated_health).await?;

		// Test loading updated node health
		let loaded_updated_health = state.load_node_health(&node_health.peer_id, node_health.epoch).await?.unwrap();
		assert_eq!(loaded_updated_health, updated_health, "Updated node health was not correctly stored or retrieved");

		// Test loading non-existent node health
		let non_existent = state.load_node_health("non_existent_peer", 999).await?;
		assert!(non_existent.is_none(), "Loading non-existent node health should return None");

		Ok(())
	}

	#[tokio::test]
	async fn test_node_health_manager() -> Result<(), Error> {
		let db_pool_conn = Database::get_test_connection().await?;
		let manager = NodeHealthManager;

		let node_health = create_test_node_health().await;

		// Test storing node health
		manager.store_node_health(&node_health, &db_pool_conn).await?;

		// Test loading node health
		let loaded_health = manager.load_node_health(&node_health.peer_id, node_health.epoch, &db_pool_conn).await?;
		assert!(loaded_health.is_some(), "Failed to load stored node health");
		let loaded_health = loaded_health.unwrap();
		assert_eq!(loaded_health, node_health, "Loaded node health does not match stored node health");

		// Test updating node health
		let mut updated_health = node_health.clone();
		updated_health.uptime_percentage = 99.8;
		updated_health.transaction_count = 1500;
		manager.update_node_health(&updated_health, &db_pool_conn).await?;

		// Test loading updated node health
		let loaded_updated_health = manager.load_node_health(&node_health.peer_id, node_health.epoch, &db_pool_conn).await?.unwrap();
		assert_eq!(loaded_updated_health, updated_health, "Updated node health was not correctly stored or retrieved");

		// Test calculating health score
		let health_score = manager.calculate_health_score(&node_health.peer_id, node_health.epoch, &db_pool_conn).await?;
		assert!(health_score > 0.0 && health_score <= 1.0, "Health score should be between 0 and 1");

		// Test loading non-existent node health
		let non_existent = manager.load_node_health("non_existent_peer", 999, &db_pool_conn).await?;
		assert!(non_existent.is_none(), "Loading non-existent node health should return None");

		Ok(())
	}

	#[tokio::test]
	async fn test_node_health_state_pg() -> Result<(), Error> {
		let db_pool_conn = Database::get_test_connection().await?;
		
		if let DbTxConn::POSTGRES(pg_conn) = &db_pool_conn {
			let pg_state = StatePg { pg: pg_conn };
			test_node_health_state(&pg_state).await?;
		} else {
			panic!("Expected PostgreSQL connection");
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_node_health_state_cas() -> Result<(), Error> {
		let db_pool_conn = Database::get_test_connection().await?;
		
		if let DbTxConn::CASSANDRA(session) = &db_pool_conn {
			let cas_state = StateCas { session: session.clone() };
			test_node_health_state(&cas_state).await?;
		} else {
			panic!("Expected Cassandra connection");
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_node_health_state_rocks() -> Result<(), Error> {
		let db_pool_conn = Database::get_test_connection().await?;
		
		if let DbTxConn::ROCKSDB(db_path) = &db_pool_conn {
			let db_path = format!("{}/node_health_test", db_path);
			let db = rocksdb::DB::open_default(&db_path)?;
			let rocks_state = StateRock { db_path: db_path.clone(), db: db.into() };
			test_node_health_state(&rocks_state).await?;
			
			// Clean up the test database
			std::fs::remove_dir_all(db_path)?;
		} else {
			panic!("Expected RocksDB connection");
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_node_health_edge_cases() -> Result<(), Error> {
		let db_pool_conn = Database::get_test_connection().await?;
		let manager = NodeHealthManager;

		// Test with extreme values
		let extreme_health = NodeHealth {
			measured_peer_id: "extreme_peer".to_string(),
			peer_id: "extreme_peer".to_string(),
			epoch: u64::MAX,
			joined_epoch: u64::MAX,
			uptime_percentage: 100.0,
			response_time_ms: u64::MAX,
			transaction_count: u64::MAX,
			block_proposal_count: u64::MAX,
			anomaly_score: 1.0,
			node_health_version: u32::MAX,
		};

		manager.store_node_health(&extreme_health, &db_pool_conn).await?;
		let loaded_extreme = manager.load_node_health(&extreme_health.peer_id, extreme_health.epoch, &db_pool_conn).await?.unwrap();
		assert_eq!(loaded_extreme, extreme_health, "Extreme values were not correctly stored or retrieved");

		// Test with minimum values
		let min_health = NodeHealth {
			measured_peer_id: "min_peer".to_string(),
			peer_id: "min_peer".to_string(),
			epoch: 0,
			joined_epoch: 0,
			uptime_percentage: 0.0,
			response_time_ms: 0,
			transaction_count: 0,
			block_proposal_count: 0,
			anomaly_score: 0.0,
			node_health_version: 0,
		};

		manager.store_node_health(&min_health, &db_pool_conn).await?;
		let loaded_min = manager.load_node_health(&min_health.peer_id, min_health.epoch, &db_pool_conn).await?.unwrap();
		assert_eq!(loaded_min, min_health, "Minimum values were not correctly stored or retrieved");

		Ok(())
	}
}