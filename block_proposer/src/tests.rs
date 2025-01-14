#[cfg(test)]
mod tests {
	use crate::{block_proposer_manager::*, block_proposer_state::*};
	use anyhow::{anyhow, Error};
	use cluster::{cluster_manager::ClusterManager, cluster_state::ClusterState};
	use db::db::{Database, DbTxConn};
	use l1x_vrf::{common::SecpVRF, secp_vrf::KeySpace};
	use node_info::{
		node_info_manager::NodeInfoManager,
		node_info_state::NodeInfoState,
	};
	use primitives::{Address, BlockNumber, Epoch};
	use serde::{Deserialize, Serialize};
	use std::collections::{BTreeMap, HashMap};
	use system::{
		account::Account,
		block_proposer::BlockProposer,
		config::Config,
		node_info::{NodeInfo, NodeInfoSignPayload},
	};
	#[derive(Serialize, Deserialize)]
	struct PayloadSignBlockProposer {
		pub cluster_address: Vec<u8>,
		pub block_number: BlockNumber,
		pub address: Address,
	}

	// Helper function to create a new AccountState for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	async fn perform_table_cleanup(cluster_address: Address) {
		let (db_pool_conn, config) = database_conn().await.unwrap();

		let node_info_state = NodeInfoState::new(&db_pool_conn).await.unwrap();
		truncate_node_info_table(&node_info_state).await.unwrap();

		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();
		truncate_block_proposer(&block_proposer_state).await.unwrap();

		let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();
		truncate_cluster_table(&cluster_state).await.unwrap();
		// Create a ClusterManager instance
		let mut cluster_manager = ClusterManager {};
		// Call the create_cluster method
		cluster_manager.create_cluster(&cluster_address, &cluster_state).await.unwrap();
	}

	pub async fn truncate_block_proposer<'a>(
		block_proposer_state: &BlockProposerState<'a>,
	) -> Result<(), Error> {
		match block_proposer_state.raw_query("TRUNCATE block_proposer;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("Truncate table failed")),
		};
		Ok(())
	}

	pub async fn truncate_cluster_table<'a>(cluster_state: &ClusterState<'a>) -> Result<(), Error> {
		match cluster_state.raw_query("TRUNCATE cluster;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("Truncate table failed")),
		};
		Ok(())
	}

	pub async fn truncate_node_info_table<'a>(
		node_info_state: &NodeInfoState<'a>,
	) -> Result<(), Error> {
		match node_info_state.raw_query("TRUNCATE node_info;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("Truncate table failed")),
		};
		Ok(())
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_block_proposer_and_load_block_proposer() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let cluster_address = [10u8; 20];
		perform_table_cleanup(cluster_address.clone()).await;

		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();

		let address = Address::from([1; 20]);
		let epoch = 1;
		let block_proposer = BlockProposer { address, cluster_address, epoch };
		// Store block proposer
		block_proposer_state
			.store_block_proposer(cluster_address, epoch, address.clone())
			.await
			.expect("Failed to store block proposers");

		// Load block proposer
		let loaded_proposer = block_proposer_state
			.load_block_proposer(cluster_address, epoch)
			.await
			.unwrap_or_else(|err| panic!("Failed to load block proposer: {}", err));

		assert_eq!(loaded_proposer, Some(block_proposer));
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_block_proposers_and_load_block_proposers() {
		let cluster_address = [10u8; 20];
		perform_table_cleanup(cluster_address.clone()).await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();

		let epoch1 = 1;
		let epoch2 = 2;
		let address1 = [1; 20];
		let address2 = [2; 20];

		let mut block_proposers: HashMap<Epoch, BlockProposer> = HashMap::new();
		block_proposers.insert(
			epoch1,
			BlockProposer {
				cluster_address,
				epoch: epoch1,
				address: address1.clone(),
			},
		);
		block_proposers.insert(
			epoch2,
			BlockProposer {
				cluster_address,
				epoch: epoch2,
				address: address2.clone(),
			},
		);

		let mut block_proposers_store: HashMap<Epoch, Address> = HashMap::new();
		block_proposers_store.insert(epoch1, address1.clone());
		block_proposers_store.insert(epoch2, address2.clone());

		let mut cluster_block_proposers = HashMap::new();
		cluster_block_proposers.insert(cluster_address, block_proposers_store.clone());

		// Store block proposers
		block_proposer_state
			.store_block_proposers(&cluster_block_proposers, None, None)
			.await
			.unwrap();

		// Load block proposers
		let loaded_proposers =
			block_proposer_state.load_block_proposers(&cluster_address).await.unwrap();

		assert_eq!(loaded_proposers, block_proposers);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_load_last_block_proposer_block_number() {
		// Define the cluster nonce
		let cluster_address = [10u8; 20];
		perform_table_cleanup(cluster_address.clone()).await;
		// Create a new BlockProposerState
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();

		// Store block proposers for the cluster_address
		block_proposer_state
			.store_block_proposer(cluster_address, 1, [1; 20])
			.await
			.unwrap();
		block_proposer_state
			.store_block_proposer(cluster_address, 2, [2; 20])
			.await
			.unwrap();
		block_proposer_state
			.store_block_proposer(cluster_address, 3, [3; 20])
			.await
			.unwrap();

		// Load the last block proposer epoch
		let last_epoch = block_proposer_state
			.load_last_block_proposer_epoch(cluster_address)
			.await
			.unwrap();

		// Assert that the last block number matches the maximum block number
		assert_eq!(last_epoch, 3);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_select_block_proposers() {
		// Create a new BlockProposerState
		let cluster_address = [10u8; 20];
		let pool_address = [21u8; 20];
		perform_table_cleanup(cluster_address.clone()).await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();
		let node_info_state = NodeInfoState::new(&db_pool_conn).await.unwrap();

		// Create a verifying key and signature

		let key_space_1 = KeySpace::new();
		let public_key_1 = key_space_1.public_key;
		let address_1 = Account::address(&public_key_1.serialize().to_vec().clone()).unwrap();
		let payload_1 = NodeInfoSignPayload {
			ip_address: "127.0.0.1".as_bytes().to_vec(),
			metadata: "Node 1".as_bytes().to_vec(),
			cluster_address: cluster_address.clone(),
		};

		let signature_1 = payload_1
			.sign_with_ecdsa(key_space_1.secret_key)
			.unwrap()
			.serialize_compact()
			.to_vec();

		let key_space_2 = KeySpace::new();
		let public_key_2 = key_space_2.public_key;
		let address_2 = Account::address(&public_key_2.serialize().to_vec().clone()).unwrap();
		let payload_2 = NodeInfoSignPayload {
			ip_address: "127.0.0.1".as_bytes().to_vec(),
			metadata: "Node 2".as_bytes().to_vec(),
			cluster_address: cluster_address.clone(),
		};

		let signature_2 = payload_2
			.sign_with_ecdsa(key_space_2.secret_key)
			.unwrap()
			.serialize_compact()
			.to_vec();

		let key_space_3 = KeySpace::new();
		let public_key_3 = key_space_3.public_key;
		let address_3 = Account::address(&public_key_3.serialize().to_vec().clone()).unwrap();
		let payload_3 = NodeInfoSignPayload {
			ip_address: "127.0.0.1".as_bytes().to_vec(),
			metadata: "Node 3".as_bytes().to_vec(),
			cluster_address: cluster_address.clone(),
		};

		let signature_3 = payload_3
			.sign_with_ecdsa(key_space_3.secret_key)
			.unwrap()
			.serialize_compact()
			.to_vec();
		let mut node_health = BTreeMap::new();
		node_health.insert(1 as u64,vec![1 as u8]);

		// Join multiple nodes to the cluster
		let node1 = NodeInfo::new(
			address_1,
			"Node 1".as_bytes().to_vec(),
			node_health.clone(),
			"127.0.0.1".as_bytes().to_vec(),
			"Node 1 metadata".as_bytes().to_vec(),
			cluster_address.clone(),
			signature_1,
			public_key_1.serialize().to_vec(),
		);
		let node2 = NodeInfo::new(
			address_2,
			"Node 2".as_bytes().to_vec(),
			node_health.clone(),
			"127.0.0.1".as_bytes().to_vec(),
			"Node 2 metadata".as_bytes().to_vec(),
			cluster_address.clone(),
			signature_2,
			public_key_2.serialize().to_vec(),
		);
		let node3 = NodeInfo::new(
			address_3,
			"Node 3".as_bytes().to_vec(),
			node_health.clone(),
			"127.0.0.1".as_bytes().to_vec(),
			"Node 3 metadata".as_bytes().to_vec(),
			cluster_address.clone(),
			signature_3,
			public_key_3.serialize().to_vec(),
		);

		let mut node_info_manager = NodeInfoManager {};

		node_info_manager
			.join_cluster(&node1.clone(), &cluster_address, &node_info_state)
			.await
			.unwrap();
		node_info_manager
			.join_cluster(&node2.clone(), &cluster_address, &node_info_state)
			.await
			.unwrap();
		node_info_manager
			.join_cluster(&node3.clone(), &cluster_address, &node_info_state)
			.await
			.unwrap();

		// Store one of the nodes as the block proposer
		block_proposer_state
			.store_block_proposer(cluster_address, 1, node1.address.clone())
			.await
			.unwrap();

		// Create a BlockProposerManager
		let mut block_proposer_manager = BlockProposerManager {};

		// Define the number of epochs to select proposers for
		let block_number: BlockNumber = 5;
		let n: Epoch = 5;
		let block_proposer =
			BlockProposer { cluster_address, epoch: 1, address: node1.address.clone() };

		let proposer_signature = block_proposer
			.sign_with_ecdsa(key_space_1.secret_key)
			.unwrap()
			.serialize_compact()
			.to_vec();

		if let Err(error) = block_proposer_manager
			.select_block_proposers(pool_address, block_proposer.clone(), block_number, &db_pool_conn,  n)
			.await
		{
			println!("Error selecting block proposers: {:?}", error);
			panic!("Failed to select block proposers");
		}

		// Load the block proposers for the cluster
		let loaded_block_proposers =
			block_proposer_state.load_block_proposers(&cluster_address).await.unwrap();

		// Assert that the loaded block proposers contain the selected node for the next n blocks
		let last_epoch = 1;
		let expected_block_proposers: HashMap<Epoch, BlockProposer> = (last_epoch..
			n + last_epoch + 1)
			.map(|i| {
				(
					i,
					BlockProposer {
						cluster_address,
						epoch: i,
						address: node1.address.clone(),
					},
				)
			})
			.collect();

		assert_eq!(loaded_block_proposers.len(), 6);
		assert_eq!(expected_block_proposers.len(), 6);
		//println!("{:?} - {:?}", loaded_block_proposers, expected_block_proposers);
		//assert_eq!(loaded_block_proposers, expected_block_proposers);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_is_block_proposer() {
		let cluster_address = [10u8; 20];
		perform_table_cleanup(cluster_address.clone()).await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();

		let epoch: Epoch = 5;

		// Create a BlockProposer representing the block proposer
		let account1 = Address::from([0; 20]);
		let block_proposer = BlockProposer { cluster_address, epoch, address: account1 };

		// Store the block proposer for the given cluster_address and epoch
		block_proposer_state
			.store_block_proposer(cluster_address, epoch, block_proposer.address.clone())
			.await
			.unwrap();

		// Create a NodeInfo representing the non-block proposer
		let account2 = Address::from([1; 20]);
		// Create another BlockProposer that is not the block proposer
		let non_proposer = BlockProposer { cluster_address, epoch, address: account2 };
		let mut block_proposer_manager = BlockProposerManager {};
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_load_selectors_block_numbers() {
		let cluster_address = [10u8; 20];
		perform_table_cleanup(cluster_address.clone()).await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();

		// Create a test address for the function
		let address = [1u8; 20];

		block_proposer_state
			.store_block_proposer(cluster_address, 100, address.clone())
			.await
			.unwrap();
		block_proposer_state
			.store_block_proposer(cluster_address, 200, address.clone())
			.await
			.unwrap();
		block_proposer_state
			.store_block_proposer(cluster_address, 300, address.clone())
			.await
			.unwrap();

		let result = block_proposer_state.load_selectors_block_epochs(&address).await;

		// Assert the expected result
		let expected_block_numbers = Some(vec![100, 200, 300]);
		// Compare the Result to the expected value
		match result {
			Ok(actual) => {
				assert_eq!(actual, expected_block_numbers);
			},
			Err(_) => {
				// Handle the error case if necessary
				panic!("Error occurred: {:?}", result);
			},
		}
	}
}
