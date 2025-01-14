#[cfg(test)]
mod tests {
	use crate::{block_manager::BlockManager};
	use account::account_state::AccountState;
	use anyhow::{anyhow, Error};
	use block_proposer::block_proposer_state::BlockProposerState;
	use cluster::{cluster_manager::ClusterManager, cluster_state::ClusterState};
	use db::{
		db::{Database, DbTxConn},
	};
	use l1x_vrf::{
		common::{ByteOps, SecpVRF},
		secp_vrf::KeySpace,
	};
	use node_info::node_info_state::NodeInfoState;
	use primitives::*;
	use serde::{Deserialize, Serialize};
	use std::time::{SystemTime, UNIX_EPOCH};
	use system::{
		account::{Account, AccountType},
		block::{Block, BlockType},
		block_header::BlockHeader,
		block_proposer::BlockProposer,
		chain_state::ChainState,
		config::Config,
		node_info::NodeInfo,
		transaction::{Transaction, TransactionType},
		transaction_receipt::TransactionReceiptResponse,
	};

	use crate::block_state::BlockState;

	#[derive(Serialize, Deserialize)]
	struct SigningPayload {
		address: Address,
		ip_address: Vec<u8>,
		metadata: Vec<u8>,
	}

	// Helper function to create a new AccountState for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	async fn perform_table_cleanup() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		let node_info_state = NodeInfoState::new(&db_pool_conn).await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();
		let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();

		truncate_account_table(&account_state).await.unwrap();
		truncate_block_table(&block_state).await.unwrap();
		truncate_cluster_table(&cluster_state).await.unwrap();
		truncate_node_info_table(&node_info_state).await.unwrap();
		truncate_block_proposer_table(&block_proposer_state).await.unwrap();

		let config_data = Config::default();
		if config_data.db_type != 2 {
			let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();
		}

		// Create a ClusterManager instance
		let mut cluster_manager = ClusterManager {};

		// Call the create_cluster method
		let cluster_address = [10u8; 20];
		cluster_manager.create_cluster(&cluster_address, &cluster_state).await.unwrap();
	}

	async fn create_account<'a>(address: Address, account_state: &AccountState<'a>) -> Account {
		let mut account = Account::new(address);
		account.balance = 5000;

		account_state.create_account(&account).await.unwrap();

		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
		assert_eq!(loaded_account.account_type, AccountType::User);
		account
	}

	pub async fn truncate_account_table<'a>(account_state: &AccountState<'a>) -> Result<(), Error> {
		match account_state.raw_query("TRUNCATE account;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("TRUNCATE table failed")),
		};
		Ok(())
	}

	pub async fn truncate_block_table<'a>(block_state: &BlockState<'a>) -> Result<(), Error> {
		match block_state.raw_query("TRUNCATE block_meta_info;").await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("TRUNCATE table failed: {}", e)),
		};
		match block_state.raw_query("TRUNCATE block_header;").await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("TRUNCATE table failed: {}", e)),
		};
		match block_state.raw_query("TRUNCATE block_transaction;").await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("TRUNCATE table failed: {}", e)),
		};
		match block_state.raw_query("TRUNCATE block_head;").await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("TRUNCATE table failed: {}", e)),
		};
		Ok(())
	}

	pub async fn truncate_node_info_table<'a>(
		node_info_state: &NodeInfoState<'a>,
	) -> Result<(), Error> {
		match node_info_state.raw_query("TRUNCATE node_info;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("TRUNCATE table failed")),
		};
		Ok(())
	}

	pub async fn truncate_cluster_table<'a>(cluster_state: &ClusterState<'a>) -> Result<(), Error> {
		match cluster_state.raw_query("TRUNCATE cluster;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("TRUNCATE table failed")),
		};
		Ok(())
	}

	pub async fn truncate_block_proposer_table<'a>(
		block_proposer_state: &BlockProposerState<'a>,
	) -> Result<(), Error> {
		match block_proposer_state.raw_query("TRUNCATE block_proposer;").await {
			Ok(_) => {},
			Err(_) => return Err(anyhow!("TRUNCATE table failed")),
		};
		Ok(())
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_and_load_chain_state() {
		// Create a sample BlockState instance
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		// Define the cluster address and block number
		let cluster_address = [10u8; 20];
		let block_number = 1;
		let block_hash = [1u8; 32];

		// Store the last block number
		let store_result = block_state
			.store_block_head(&cluster_address, block_number, block_hash.clone())
			.await;

		assert!(store_result.is_ok());

		// Load the last block number
		let load_result = block_state.load_chain_state(cluster_address.clone()).await;

		// Assert that the loaded block number matches the expected value
		let chain_state = ChainState { cluster_address, block_number, block_hash };
		assert_eq!(load_result.unwrap(), chain_state);

		// if let StateInternalImpl::StatePg(_) = block_state.get_state() {
		// 	match drop_database(&block_state).await {
		// 		Ok(_) => println!("Database dropped successfully"),
		// 		Err(e) => eprintln!("Failed to drop database: {:?}", e),
		// 	}
		// }
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn load_chain_state_not_found() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let cluster_address = [10u8; 20];

		let result = block_state.load_chain_state(cluster_address).await.unwrap();

		assert_eq!(result.block_number, 0);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn load_block_head() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key.serialize().to_vec();
		let address = Account::address(&verifying_key).unwrap();
		let _ = create_account(address, &account_state).await;
		let payload = SigningPayload {
			address,
			ip_address: "127.0.0.1".as_bytes().to_vec(),
			metadata: "metadata".as_bytes().to_vec(),
		};
		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let signature = signature.serialize_compact().to_vec();
		// Create some sample transactions
		let transactions = get_transactions(
			address.clone(),
			signature.clone(),
			verifying_key.clone(),
			&account_state,
		)
		.await;

		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let cluster_address = [10u8; 20];
		let block_number = 12345;
		let block_hash = [1u8; 32];
		let parent_hash = [2u8; 32];
		let timestamp =
			SystemTime::now().duration_since(UNIX_EPOCH).expect("unix time error").as_secs();
		let block_header = BlockHeader::new(
			block_number,
			block_hash,
			parent_hash,
			BlockType::L1XTokenBlock,
			[10u8; 20],
			timestamp,
			3,
			2,
			[0; 32],
			1,
		);
		let block = Block::new(block_header, transactions); // Create a dummy block for testing

		block_state.store_block(block.clone()).await.unwrap();

		let loaded_block = block_state.block_head(cluster_address).await.unwrap();
		//println!("loaded_block: {:?}", loaded_block);
		assert_eq!(block.transactions.len(), loaded_block.transactions.len());
	}

	async fn get_transactions<'a>(
		address: Address,
		signature: SignatureBytes,
		verifying_key: VerifyingKeyBytes,
		account_state: &AccountState<'a>,
	) -> Vec<Transaction> {
		let recipient = [1u8; 20];
		let current_nonce = account_state.get_nonce(&address).await.unwrap();
		vec![
			Transaction {
				nonce: current_nonce + 1,
				transaction_type: TransactionType::NativeTokenTransfer(recipient, 100),
				fee_limit: 5,
				signature: signature.clone(),
				verifying_key: verifying_key.clone(),
				eth_original_transaction: None,
			},
			Transaction {
				nonce: current_nonce + 2,
				transaction_type: TransactionType::NativeTokenTransfer(recipient, 200),
				fee_limit: 5,
				signature: signature.clone(),
				verifying_key: verifying_key.clone(),
				eth_original_transaction: None,
			},
			Transaction {
				nonce: current_nonce + 3,
				transaction_type: TransactionType::NativeTokenTransfer(recipient, 300),
				fee_limit: 5,
				signature: signature.clone(),
				verifying_key: verifying_key.clone(),
				eth_original_transaction: None,
			},
		]
	}

	fn get_block_hash() -> BlockHash {
		[
			89, 43, 118, 155, 0, 206, 147, 7, 183, 70, 3, 67, 66, 103, 136, 103, 88, 33, 19, 7,
			131, 161, 87, 86, 30, 218, 251, 171, 58, 7, 228, 18,
		]
	}

	// async fn load_blocks_in_db(block_state: BlockState) {
	//     for i in 0..4 {
	//         let node_info_state = create_test_node_info_state().await.unwrap();
	//         let block_proposer_state = create_test_block_proposer_state().await.unwrap();
	//         let account_state = create_test_account_state().await.unwrap();

	//         let key_space = KeySpace::new();
	//         let verifying_key = key_space.public_key.serialize().to_vec();
	//         let address = Account::address(&verifying_key).unwrap();
	//         let _ = create_account(address, &account_state).await;

	//         let payload = SigningPayload {
	//             address,
	//             ip_address: "127.0.0.1".as_bytes().to_vec(),
	//             metadata: "metadata".as_bytes().to_vec(),
	//         };

	//         let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
	//         let signature = signature.serialize_compact().to_vec();
	//         // Create some sample transactions
	//         let transactions = get_transactions(
	//             address.clone(),
	//             signature.clone(),
	//             verifying_key.clone(),
	//             &account_state,
	//         )
	//         .await;

	//         let cluster_address = [10u8; 20];
	//         let block_number = i + 1;
	//         let node_info = NodeInfo {
	//             address,
	//             ip_address: "127.0.0.1".as_bytes().to_vec(),
	//             metadata: "metadata".as_bytes().to_vec(),
	//             signature,
	//             verifying_key,
	//             timestamp: SystemTime::now()
	//                 .duration_since(UNIX_EPOCH)
	//                 .unwrap()
	//                 .as_millis(),
	//         };

	//         node_info_state
	//             .store_node_info(&cluster_address, &address, &node_info.clone())
	//             .await
	//             .unwrap();
	//         // Store block proposer
	//         block_proposer_state
	//             .store_block_proposer(cluster_address, block_number, node_info.address.clone())
	//             .await
	//             .expect("Failed to store block proposers");

	//         let block_proposer = BlockProposer {
	//             cluster_address,
	//             block_number,
	//             address,
	//         };
	//         // Call the new_block function
	//         let block_manager = BlockManager::new(CONNECTION_STRING);
	//         let result = block_manager
	//             .new_block(transactions.clone(), block_proposer)
	//             .await;
	//     }
	// }

	// #[tokio::test]
	// #[serial_test::serial]
	// async fn test_load_latest_block_headers() {
	//     let block_state = create_test_block_state().await.unwrap();
	//     load_blocks_in_db().await;
	// }

	#[tokio::test]
	#[serial_test::serial]
	async fn test_load_latest_transactions() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key.serialize().to_vec();
		let address = Account::address(&verifying_key).unwrap();
		let _ = create_account(address, &account_state).await;
		let payload = SigningPayload {
			address,
			ip_address: "127.0.0.1".as_bytes().to_vec(),
			metadata: "metadata".as_bytes().to_vec(),
		};
		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let signature = signature.serialize_compact().to_vec();
		// Create some sample transactions
		let transactions = get_transactions(
			address.clone(),
			signature.clone(),
			verifying_key.clone(),
			&account_state,
		)
		.await;

		block_state
			.store_transaction(1, [0u8; 32], transactions[0].clone(), 1)
			.await
			.unwrap();
		block_state
			.store_transaction(2, [1u8; 32], transactions[1].clone(), 2)
			.await
			.unwrap();
		block_state
			.store_transaction(3, [2u8; 32], transactions[2].clone(), 3)
			.await
			.unwrap();

		let result = block_state.load_latest_transactions(2).await.unwrap();

		assert!(result.len() == 2);
		// if let StateInternalImpl::StatePg(_) = block_state.get_state() {
		// 	match drop_database(&block_state).await {
		// 		Ok(_) => println!("Database dropped successfully"),
		// 		Err(e) => eprintln!("Failed to drop database: {:?}", e),
		// 	}
		// }
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_new_block() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		let node_info_state = NodeInfoState::new(&db_pool_conn).await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key.serialize().to_vec();
		let address = Account::address(&verifying_key).unwrap();
		let _ = create_account(address, &account_state).await;

		let payload = SigningPayload {
			address,
			ip_address: "127.0.0.1".as_bytes().to_vec(),
			metadata: "metadata".as_bytes().to_vec(),
		};

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let signature = signature.serialize_compact().to_vec();
		// Create some sample transactions
		let transactions = get_transactions(
			address.clone(),
			signature.clone(),
			verifying_key.clone(),
			&account_state,
		)
		.await;
		let ip_address = "127.0.0.1".as_bytes().to_vec();
		let metadata = "metadata".as_bytes().to_vec();
		let cluster_address = [10u8; 20];
		let block_number = 1;
		let epoch = 1 as u64;
		let node_info =
			NodeInfo::new(address, ip_address, metadata, cluster_address, signature, verifying_key);

		node_info_state.store_node_info(&node_info.clone()).await.unwrap();
		// Store block proposer
		block_proposer_state
			.store_block_proposer(cluster_address, block_number, node_info.address.clone())
			.await
			.expect("Failed to store block proposers");

		let block_proposer = BlockProposer { cluster_address, epoch, address };
		// Call the new_block function
		let block_manager = BlockManager::new();
		let result = block_manager
			.create_regular_block(transactions.clone(), block_proposer, &block_state, &account_state)
			.await;
		println!("{:?}", result);
		// Assert that the result is successful
		assert!(result.is_ok());

		// Assert that the new block and block header are created and stored correctly
		let new_block = result.unwrap();

		// Assert the block number, hash, parent hash, timestamp, and transactions
		assert_eq!(new_block.block_header.block_number, 1);
		//assert_eq!(new_block.block_hash, get_block_hash());
		assert_eq!(new_block.block_header.parent_hash, [0u8; 32]);
		//assert_eq!(new_block.timestamp, /* expected timestamp */);
		assert_eq!(new_block.transactions, transactions);

		// Assert the block header details
		//assert_eq!(new_block_header.block_number, 1);
		//assert_eq!(new_block_header.block_hash, get_block_hash());
		//assert_eq!(new_block_header.parent_hash, [0u8; 32]);
		//assert_eq!(new_block_header.timestamp, /* expected timestamp */);
		// if let StateInternalImpl::StatePg(_) = block_state.get_state() {
		// 	match drop_database(&block_state).await {
		// 		Ok(_) => println!("Database dropped successfully"),
		// 		Err(e) => eprintln!("Failed to drop database: {:?}", e),
		// 	}
		// }
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_compute_block_hash() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key.serialize().to_vec();
		let address = Account::address(&verifying_key).unwrap();
		let _ = create_account(address, &account_state).await;
		let payload = SigningPayload {
			address,
			ip_address: "127.0.0.1".as_bytes().to_vec(),
			metadata: "metadata".as_bytes().to_vec(),
		};
		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let signature = signature.serialize_compact().to_vec();
		// Create some sample transactions
		let transactions = get_transactions(
			address.clone(),
			signature.clone(),
			verifying_key.clone(),
			&account_state,
		)
		.await;

		// Serialize the transactions
		let transactions_bytes: Vec<u8> = transactions.to_bytes().unwrap();
		let block_manager = BlockManager::new();
		// Call the compute_block_hash function
		let block_hash = block_manager.compute_block_hash(&transactions_bytes);

		println!("{:?}", block_hash);
		// Assert the computed block hash matches the expected value
		//assert_eq!(block_hash, get_block_hash());
	}

	/// This should test batch_store_block_headers() as well, since it is called inside
	/// batch_store_blocks()
	#[tokio::test]
	#[serial_test::serial]
	async fn test_batch_store_blocks() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let cluster_address = [1u8; 20];

		let public_key_hex = "03163005b5bd11c0d9470113cd1f46ae002412fbe819c0cb284e3808bb7449673c";

		let v_key_tx1 = hex::decode(public_key_hex).unwrap();
		let address = Account::address(&v_key_tx1).unwrap();
		let signature = [1u8; 20].to_vec();

		let txs_1 =
			get_transactions(address, signature.clone(), v_key_tx1.clone(), &account_state).await;
		let txs_2 = get_transactions(address, signature, v_key_tx1, &account_state).await;

		let timestamp1 =
			SystemTime::now().duration_since(UNIX_EPOCH).expect("unix time error").as_secs();
		// Create a block
		let block_1 = Block {
			block_header: BlockHeader {
				block_number: 1,
				block_hash: [1u8; 32],
				parent_hash: [0u8; 32],
				block_type: BlockType::L1XTokenBlock,
				cluster_address,
				timestamp: timestamp1,
				num_transactions: 3,
				block_version: 2,
				state_hash: [0; 32],
				epoch: 1,
			},
			transactions: txs_1.clone(),
		};
		let timestamp2 =
			SystemTime::now().duration_since(UNIX_EPOCH).expect("unix time error").as_secs();

		let block_2 = Block {
			block_header: BlockHeader {
				block_number: 2,
				block_hash: [2u8; 32],
				parent_hash: block_1.block_header.block_hash,
				block_type: BlockType::L1XTokenBlock,
				cluster_address,
				timestamp: timestamp2,
				num_transactions: 3,
				block_version: 2,
				state_hash: [0; 32],
				epoch:1,
			},
			transactions: txs_2.clone(),
		};

		let blocks = vec![block_1.clone(), block_2.clone()];

		let result = block_state.batch_store_blocks(blocks).await;
		assert!(result.is_ok());

		// Test load_transaction function
		let result = block_state
			.load_block(block_1.block_header.block_number, &cluster_address)
			.await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), block_1);

		let result = block_state
			.load_block(block_2.block_header.block_number, &cluster_address)
			.await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), block_2);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_and_load_transaction() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let public_key_hex = "03163005b5bd11c0d9470113cd1f46ae002412fbe819c0cb284e3808bb7449673c";

		let v_key_tx1 = hex::decode(public_key_hex).unwrap();

		// Create a test BlockNumber and Transaction
		let block_number = 42;
		let recipient = [1u8; 20];
		let block_hash = [13u8; 32];
		let transaction = Transaction {
			nonce: 1,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, 100),
			fee_limit: 5,
			signature: [1u8; 20].to_vec(),
			verifying_key: v_key_tx1,
			eth_original_transaction: None,
		};

		// Test store_transaction function
		let result = block_state
			.store_transaction(block_number, block_hash, transaction.clone(), 1)
			.await;
		assert!(result.is_ok());

		// Create the expected transaction hash for the load_transaction test
		let expected_transaction_hash = transaction.transaction_hash().unwrap();

		// Test load_transaction function
		let result = block_state.load_transaction(expected_transaction_hash).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap().unwrap(), transaction);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_batch_store_and_load_transaction() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let public_key_hex = "03163005b5bd11c0d9470113cd1f46ae002412fbe819c0cb284e3808bb7449673c";

		let v_key_tx1 = hex::decode(public_key_hex).unwrap();

		// Create a test BlockNumber and Transaction
		let block_number_1 = 42;
		let recipient = [1u8; 20];
		let block_hash_1 = [13u8; 32];
		let transaction_1 = Transaction {
			nonce: 1,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, 100),
			fee_limit: 5,
			signature: [1u8; 20].to_vec(),
			verifying_key: v_key_tx1.clone(),
			eth_original_transaction: None,
		};

		let block_number_2 = 42;
		let recipient = [1u8; 20];
		let block_hash_2 = [14u8; 32];
		let transaction_2 = Transaction {
			nonce: 2,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, 102),
			fee_limit: 4,
			signature: [1u8; 20].to_vec(),
			verifying_key: v_key_tx1,
			eth_original_transaction: None,
		};

		let transactions = vec![
			(block_number_1, block_hash_1, transaction_1.clone()),
			(block_number_2, block_hash_2, transaction_2.clone()),
		];

		// Test store_transaction function
		let result = block_state.batch_store_transactions(transactions).await;
		assert!(result.is_ok());

		// Create the expected transaction hash for the load_transaction test
		let expected_transaction_1_hash = transaction_1.transaction_hash().unwrap();
		let expected_transaction_2_hash = transaction_2.transaction_hash().unwrap();

		// Test load_transaction function
		let result = block_state.load_transaction(expected_transaction_1_hash).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap().unwrap(), transaction_1);

		let result = block_state.load_transaction(expected_transaction_2_hash).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap().unwrap(), transaction_2);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_and_load_transaction_receipt_response() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let public_key_hex = "03163005b5bd11c0d9470113cd1f46ae002412fbe819c0cb284e3808bb7449673c";

		let v_key_tx1 = hex::decode(public_key_hex).unwrap();
		// Create a test BlockNumber and Transaction
		let block_number = 42;
		let recipient = [1u8; 20];
		let block_hash = [13u8; 32];
		let transaction = Transaction {
			nonce: 1,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, 100),
			fee_limit: 5,
			signature: [1u8; 20].to_vec(),
			verifying_key: v_key_tx1,
			eth_original_transaction: None,
		};

		// Test store_transaction function
		let result = block_state
			.store_transaction(block_number, block_hash.clone(), transaction.clone(), 1)
			.await;
		assert!(result.is_ok());

		// Create the expected transaction hash for the load_transaction test
		let expected_transaction_hash = transaction.transaction_hash().unwrap();
		let cluster_address = [10u8; 20];
		// Test load_transaction function
		let result = block_state
			.load_transaction_receipt(expected_transaction_hash, cluster_address)
			.await
			.unwrap()
			.unwrap();
		// assert!(result.is_ok());
		// let transaction_receipt_response =
		//     TransactionReceiptResponse::new(transaction, true, block_number, block_hash, 5);
		let from = Account::address(&transaction.verifying_key).unwrap();
		let trr = TransactionReceiptResponse::new(
			transaction.clone(),
			expected_transaction_hash,
			false,
			from,
			block_number,
			block_hash,
			transaction.fee_limit,
			0,
		);
		assert_eq!(trr.transaction, result.transaction);
		assert_eq!(trr.tx_hash, result.tx_hash);
		assert_eq!(trr.status, result.status);
		assert_eq!(trr.from, result.from);
		assert_eq!(trr.block_number, result.block_number);
		assert_eq!(trr.block_hash, result.block_hash);
		assert_eq!(trr.fee_used, result.fee_used);

		// assert_eq!(result.unwrap(), transaction_receipt_response);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_store_and_load_transaction_receipt_response_by_address() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;
		let public_key_hex = "03163005b5bd11c0d9470113cd1f46ae002412fbe819c0cb284e3808bb7449673c";

		let v_key_tx1 = hex::decode(public_key_hex).unwrap();
		let v_key_tx2 = hex::decode(public_key_hex).unwrap();

		// Create a test BlockNumber and Transaction
		let block_number = 42;
		let recipient = [1u8; 20];
		let block_hash = [13u8; 32];
		let transaction1 = Transaction {
			nonce: 1,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, 100),
			fee_limit: 5,
			signature: [1u8; 32].to_vec(),
			verifying_key: v_key_tx1,
			eth_original_transaction: None,
		};
		let transaction2 = Transaction {
			nonce: 2,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, 100),
			fee_limit: 5,
			signature: [1u8; 32].to_vec(),
			verifying_key: v_key_tx2,
			eth_original_transaction: None,
		};

		let public_key_bytes = hex::decode(public_key_hex).unwrap();

		let sender = Account::address(&public_key_bytes).unwrap();

		// Test store_transaction function
		let result = block_state
			.store_transaction(block_number, block_hash.clone(), transaction1.clone(), 1)
			.await;
		assert!(result.is_ok());
		let result = block_state
			.store_transaction(block_number, block_hash.clone(), transaction2.clone(), 2)
			.await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_get_transaction_count() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup().await;

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key.serialize().to_vec();
		let address = Account::address(&verifying_key).unwrap();
		let _ = create_account(address, &account_state).await;
		let payload = SigningPayload {
			address,
			ip_address: "127.0.0.1".as_bytes().to_vec(),
			metadata: "metadata".as_bytes().to_vec(),
		};
		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let signature = signature.serialize_compact().to_vec();
		// Create some sample transactions
		let transactions = get_transactions(
			address.clone(),
			signature.clone(),
			verifying_key.clone(),
			&account_state,
		)
		.await;

		let block_state = BlockState::new(&db_pool_conn).await.unwrap();

		// let cluster_address = [10u8; 20];
		let block_number = 12345;
		let block_hash = [1u8; 32];
		let parent_hash = [2u8; 32];
		let timestamp =
			SystemTime::now().duration_since(UNIX_EPOCH).expect("unix time error").as_secs();
		let block = Block::new(
			BlockHeader::new(
				block_number,
				block_hash,
				parent_hash,
				BlockType::L1XTokenBlock,
				[10u8; 20],
				timestamp,
				3,
				2,
				[0; 32],
				1,
			),
			transactions.clone(),
		); // Create a dummy block for testing

		//println!("{:?}", block_state.store_block(block.clone()).await);
		block_state.store_block(block.clone()).await.unwrap();

		// add another block with 2 transactions only
		block_state
			.store_block(Block::new(
				BlockHeader { block_number: block_number + 1, ..block.block_header },
				transactions
					.into_iter()
					.take(2)
					.map(|txn| system::transaction::Transaction { nonce: txn.nonce + 10, ..txn })
					.collect(),
			))
			.await
			.unwrap();

		assert_eq!(5, block_state.get_transaction_count(&address, None).await.unwrap());

		assert_eq!(
			5,
			block_state
				.get_transaction_count(&address, Some(block_number + 10))
				.await
				.unwrap()
		);

		assert_eq!(
			3,
			block_state.get_transaction_count(&address, Some(block_number)).await.unwrap()
		);

		let bogus_block_number = 111111;
		assert_eq!(
			5,
			block_state
				.get_transaction_count(&address, Some(bogus_block_number))
				.await
				.unwrap()
		);
	}
}
