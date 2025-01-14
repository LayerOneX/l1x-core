#[cfg(test)]
mod tests {
	use crate::mempool::Mempool;
	use account::{account_manager::AccountManager, account_state::AccountState};
	use anyhow::Error;
	use block::block_state::BlockState;
	use block_proposer::block_proposer_state::BlockProposerState;
	use cluster::{cluster_manager::ClusterManager, cluster_state::ClusterState};
	use db::db::{Database, DbTxConn};
	use l1x_vrf::{common::SecpVRF, secp_vrf::KeySpace};
	use node_info::node_info_state::NodeInfoState;
	use primitives::*;
	use serde::{Deserialize, Serialize};
	use system::{
		account::Account,
		block_proposer::BlockProposer,
		config::Config,
		network::BroadcastNetwork,
		node_info::NodeInfo,
		transaction::{Transaction, TransactionType, TransactionTypeNativeTX},
	};
	use tokio::sync::{broadcast, mpsc};

	#[derive(Debug, Serialize, Deserialize)]
	pub struct TXNT {
		pub nonce: Nonce,
		pub transaction_type: TransactionTypeNativeTX,
		pub fee_limit: Balance,
	}

	#[derive(Serialize, Deserialize, Debug)]
	struct PayloadTX {
		pub nonce: Nonce,
		pub transaction_type: TransactionType,
		pub fee_limit: Balance,
	}

	#[derive(Serialize, Deserialize, Debug)]
	struct PayloadNode {
		pub address: Address,
		pub ip_address: IpAddress,
		pub metadata: Vec<u8>,
	}

	// Helper function to create a new State for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		println!("database_conn");
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	async fn perform_table_cleanup(cluster_address: Address) {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		//let event_state = EventState::new(&db_pool_conn).await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		let cluster_state = ClusterState::new(&db_pool_conn).await.unwrap();
		let node_info_state = NodeInfoState::new(&db_pool_conn).await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();

		truncate_account_table(&account_state).await;
		truncate_cluster_table(&cluster_state).await;
		truncate_node_info_table(&node_info_state).await;
		truncate_block_table(&block_state).await;
		truncate_block_proposer_table(&block_proposer_state).await;

		// Create a ClusterManager instance
		let mut cluster_manager = ClusterManager {};

		// Call the create_cluster method
		cluster_manager.create_cluster(&cluster_address, &cluster_state).await.unwrap();
	}

	pub async fn truncate_cluster_table<'a>(cluster_state: &ClusterState<'a>) {
		cluster_state.raw_query("TRUNCATE cluster;").await;
	}

	async fn create_account<'a>(address: Address, account_state: &AccountState<'a>) -> Account {
		let account = Account::new(address);

		account_state.create_account(&account).await.unwrap();

		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
		account
	}

	pub async fn truncate_account_table<'a>(account_state: &AccountState<'a>) {
		account_state.raw_query("TRUNCATE account;").await;
	}

	pub async fn truncate_node_info_table<'a>(node_info_state: &NodeInfoState<'a>) {
		node_info_state.raw_query("TRUNCATE node_info;").await;
	}

	pub async fn truncate_block_proposer_table<'a>(block_proposer_state: &BlockProposerState<'a>) {
		block_proposer_state.raw_query("TRUNCATE block_proposer;").await;
	}

	pub async fn truncate_block_table<'a>(block_state: &BlockState<'a>) {
		block_state.raw_query("TRUNCATE block_cluster_map;").await;
		block_state.raw_query("TRUNCATE block_head;").await;
		block_state.raw_query("TRUNCATE block_header;").await;
		block_state.raw_query("TRUNCATE block_meta_info;").await;
		block_state.raw_query("TRUNCATE block_transaction;").await;
	}

	async fn get_nonce<'a>(
		account_manager: &mut AccountManager,
		account_state: &AccountState<'a>,
	) -> Nonce {
		account_manager.get_current_nonce(account_state).await.unwrap() + 1
	}

	async fn initialize_network() -> mpsc::Sender<BroadcastNetwork> {
		let (network_client_tx, mut network_client_rx) = mpsc::channel(1000);
		network_client_tx
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_add_transaction() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;

		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			60,
			60,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);
		let recipient: Address = [12u8; 20].into();

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();
		let mut account = create_account(sender, &account_state).await;

		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();

		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 10;
		let fee_limit = 100;

		let sign_tx_payload = TXNT {
			nonce,
			transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
				recipient,
				amount.to_string(),
			),
			fee_limit,
		};
		let signature = sign_tx_payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		assert!(mempool.add_transaction(transaction, &db_pool_conn).await.is_ok());
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_add_transaction_invalid_sender() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			60,
			60,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);
		let recipient: Address = [140u8; 20].into();

		let nonce = 1;
		let amount = 10;
		let fee_limit = 100;

		let payload = TXNT {
			nonce,
			transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
				recipient,
				amount.to_string(),
			),
			fee_limit,
		};
		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		assert_eq!(
			mempool
				.add_transaction(transaction, &db_pool_conn)
				.await
				.err()
				.unwrap()
				.to_string(),
			"Invalid transaction sender"
		);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_add_transaction_full() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space_1 = KeySpace::new();
		let verifying_key_1 = key_space_1.public_key;

		let mut mempool = Mempool::new(
			1,
			1000,
			10,
			60,
			60,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space_1.secret_key,
			verifying_key_1,
			true,
		);
		let recipient1: Address = [16u8; 20].into();

		// let mut csprng = OsRng {};
		// let keypair1: Keypair = Keypair::generate(&mut csprng);

		// let verifying_key1 = keypair1.public;
		let sender1: Address = Account::address(&verifying_key_1.serialize().to_vec()).unwrap();
		let mut account1 = create_account(sender1, &account_state).await;

		account1.balance = 5000;
		account_state.update_balance(&account1).await.unwrap();
		let mut account_manager1 = AccountManager { account: account1 };

		let nonce1 = get_nonce(&mut account_manager1, &account_state).await;
		let amount1 = 10;
		let fee_limit1 = 100;

		let payload_1 = TXNT {
			nonce: nonce1,
			transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
				recipient1,
				amount1.to_string(),
			),
			fee_limit: fee_limit1,
		};

		let signature_1 = payload_1.sign_with_ecdsa(key_space_1.secret_key).unwrap();

		let transaction_1 = Transaction::new(
			nonce1,
			TransactionType::NativeTokenTransfer(recipient1, amount1),
			fee_limit1,
			signature_1,
			verifying_key_1,
		);

		let recipient2: Address = [18u8; 20].into();

		let key_space_2 = KeySpace::new();
		let verifying_key2 = key_space_2.public_key;

		let sender2: Address = Account::address(&verifying_key2.serialize().to_vec()).unwrap();

		let mut account2 = create_account(sender2, &account_state).await;
		account2.balance = 5000;
		account_state.update_balance(&account2).await.unwrap();
		let mut account_manager2 = AccountManager { account: account2 };
		let nonce2 = get_nonce(&mut account_manager2, &account_state).await;
		let amount2 = 5;
		let fee_limit2 = 100;

		let payload_2 = TXNT {
			nonce: nonce2,
			transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
				recipient2,
				amount2.to_string(),
			),
			fee_limit: fee_limit2,
		};

		let signature_2 = payload_2.sign_with_ecdsa(key_space_2.secret_key).unwrap();

		let transaction_2 = Transaction::new(
			nonce2,
			TransactionType::NativeTokenTransfer(recipient2, amount2),
			fee_limit2,
			signature_2,
			verifying_key2,
		);

		assert!(mempool.add_transaction(transaction_1.clone(), &db_pool_conn).await.is_ok());
		assert_eq!(
			mempool
				.add_transaction(transaction_2.clone(), &db_pool_conn)
				.await
				.err()
				.unwrap()
				.to_string(),
			"Mempool is full"
		);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_add_transaction_gas_exceeds_limit() {
		let cluster_address = [0; 20];
		perform_table_cleanup(cluster_address.clone()).await;
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			60,
			60,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);
		let recipient: Address = [20u8; 20].into();

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();
		let mut account = create_account(sender, &account_state).await;

		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 10;
		let fee_limit = 2000;

		let payload = PayloadTX {
			nonce,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
		};

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		assert_eq!(
			mempool
				.add_transaction(transaction, &db_pool_conn)
				.await
				.err()
				.unwrap()
				.to_string(),
			"Transaction fee exceeds fee limit"
		);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_add_transaction_invalid_signature() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			60,
			60,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);
		let recipient: Address = [22u8; 20].into();

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();

		let mut account = create_account(sender, &account_state).await;
		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 10;
		let fee_limit = 100;

		let payload = PayloadTX {
			nonce,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
		};

		let new_key_space = KeySpace::new();

		let signature = payload.sign_with_ecdsa(new_key_space.secret_key).unwrap();

		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		assert_eq!(
			mempool
				.add_transaction(transaction, &db_pool_conn)
				.await
				.err()
				.unwrap()
				.to_string(),
			"Invalid transaction signature"
		);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_insufficient_balance() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		// Create a transaction with an amount greater than the account balance
		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			60,
			60,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);
		let recipient: Address = [24u8; 20].into();

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();
		let mut account = create_account(sender, &account_state).await;
		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 10000;
		let fee_limit = 100;

		let payload = TXNT {
			nonce,
			transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
				recipient,
				amount.to_string(),
			),
			fee_limit,
		};

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		assert_eq!(
			mempool
				.add_transaction(transaction, &db_pool_conn)
				.await
				.err()
				.unwrap()
				.to_string(),
			"Insufficient balance to cover the transaction"
		);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_add_transaction_rate_limit_exceeded() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			2,
			10000,
			60000,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		); // Set rate limit to 2 transactions within 10 seconds
		let recipient: Address = [26u8; 20].into();

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();
		let mut account = create_account(sender, &account_state).await;
		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 10;
		let fee_limit = 100;

		let transaction_type_native_tx =
			TransactionTypeNativeTX::NativeTokenTransfer(recipient, amount.to_string());

		let payload = TXNT { nonce, transaction_type: transaction_type_native_tx, fee_limit };

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		// let payload = PayloadTX {
		//     nonce,
		//     transaction_type: TransactionType::NativeTokenTransfer(recipient, amount),
		//     fee_limit,
		// };
		//
		// let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		let transaction1 = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 5;
		let fee_limit = 100;

		let transaction_type_native_tx =
			TransactionTypeNativeTX::NativeTokenTransfer(recipient, amount.to_string());

		let payload =
			TXNT { nonce: nonce + 1, transaction_type: transaction_type_native_tx, fee_limit };

		// let payload = PayloadTX {
		//     nonce: nonce + 1,
		//     transaction_type: TransactionType::NativeTokenTransfer(recipient, amount),
		//     fee_limit,
		// };

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let transaction2 = Transaction::new(
			nonce + 1,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);

		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 3;
		let fee_limit = 100;

		let transaction_type_native_tx =
			TransactionTypeNativeTX::NativeTokenTransfer(recipient, amount.to_string());

		let payload =
			TXNT { nonce: nonce + 2, transaction_type: transaction_type_native_tx, fee_limit };

		// let payload = PayloadTX {
		//     nonce: nonce + 2,
		//     transaction_type: TransactionType::NativeTokenTransfer(recipient, amount),
		//     fee_limit,
		// };

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let transaction3 = Transaction::new(
			nonce + 2,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);

		assert!(mempool.add_transaction(transaction1.clone(), &db_pool_conn).await.is_ok());
		assert!(mempool.add_transaction(transaction2.clone(), &db_pool_conn).await.is_ok());
		assert_eq!(
			mempool
				.add_transaction(transaction3.clone(), &db_pool_conn)
				.await
				.err()
				.unwrap()
				.to_string(),
			"Rate limit exceeded for the sender"
		);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_remove_transaction() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			60,
			60,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);
		let recipient: Address = [28u8; 20].into();

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();
		let mut account = create_account(sender, &account_state).await;
		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 10;
		let fee_limit = 100;

		let transaction_type_native_tx =
			TransactionTypeNativeTX::NativeTokenTransfer(recipient, amount.to_string());

		let payload = TXNT { nonce, transaction_type: transaction_type_native_tx, fee_limit };

		// let payload = PayloadTX {
		//     nonce,
		//     transaction_type: TransactionType::NativeTokenTransfer(recipient, amount),
		//     fee_limit,
		// };

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		assert!(mempool.add_transaction(transaction, &db_pool_conn).await.is_ok());
		mempool.remove_transaction_by_sender(&sender).await;
		let transactions = mempool.get_transactions().await;
		assert!(transactions.is_empty());
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_get_transactions() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space1 = KeySpace::new();
		let verifying_key1 = key_space1.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			60000,
			60000,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space1.secret_key,
			verifying_key1,
			true,
		);
		let recipient1: Address = [30u8; 20].into();

		let sender1: Address = Account::address(&verifying_key1.serialize().to_vec()).unwrap();

		let mut account1 = create_account(sender1, &account_state).await;
		account1.balance = 5000;
		account_state.update_balance(&account1).await.unwrap();
		let mut account_manager1 = AccountManager { account: account1 };
		let nonce1 = get_nonce(&mut account_manager1, &account_state).await;
		let amount1 = 10;
		let fee_limit1 = 100;

		let transaction_type_native_tx =
			TransactionTypeNativeTX::NativeTokenTransfer(recipient1, amount1.to_string());

		let payload1 = TXNT {
			nonce: nonce1,
			transaction_type: transaction_type_native_tx,
			fee_limit: fee_limit1,
		};

		let signature1 = payload1.sign_with_ecdsa(key_space1.secret_key).unwrap();

		let transaction1 = Transaction::new(
			nonce1,
			TransactionType::NativeTokenTransfer(recipient1, amount1),
			fee_limit1,
			signature1,
			verifying_key1,
		);
		let recipient2: Address = [32u8; 20].into();

		let key_space2 = KeySpace::new();
		let verifying_key2 = key_space2.public_key;

		let sender2: Address = Account::address(&verifying_key2.serialize().to_vec()).unwrap();
		let mut account2 = create_account(sender2, &account_state).await;
		account2.balance = 5000;
		account_state.update_balance(&account2).await.unwrap();
		let mut account_manager2 = AccountManager { account: account2 };
		let nonce2 = get_nonce(&mut account_manager2, &account_state).await;
		let amount2 = 5;
		let fee_limit2 = 50;

		let transaction_type_native_tx =
			TransactionTypeNativeTX::NativeTokenTransfer(recipient2, amount2.to_string());

		let payload2 = TXNT {
			nonce: nonce2,
			transaction_type: transaction_type_native_tx,
			fee_limit: fee_limit2,
		};
		let signature2 = payload2.sign_with_ecdsa(key_space2.secret_key).unwrap();

		let transaction2 = Transaction::new(
			nonce2,
			TransactionType::NativeTokenTransfer(recipient2, amount2),
			fee_limit2,
			signature2,
			verifying_key2,
		);
		assert!(mempool.add_transaction(transaction1.clone(), &db_pool_conn).await.is_ok());
		assert!(mempool.add_transaction(transaction2.clone(), &db_pool_conn).await.is_ok());
		let transactions = mempool.get_transactions().await;
		assert_eq!(transactions.len(), 2);
		//println!("{:?}", transactions);
		//println!("{:?}", expected_transaction1);
		//assert!(transactions.contains(&expected_transaction1));
		//assert!(transactions.contains(&expected_transaction2));
		for transaction in transactions {
			if transaction == transaction1 {
				assert_eq!(transaction.nonce, transaction1.nonce);
				assert_eq!(
					Account::address(&transaction.verifying_key).unwrap(),
					Account::address(&transaction1.verifying_key).unwrap()
				);
				assert_eq!(transaction.transaction_type, transaction1.transaction_type);
				assert_eq!(transaction.fee_limit, transaction1.fee_limit);
				assert_eq!(transaction.signature, transaction1.signature);
				assert_eq!(transaction.verifying_key, transaction1.verifying_key);
			} else if transaction == transaction2 {
				assert_eq!(transaction.nonce, transaction2.nonce);
				assert_eq!(
					Account::address(&transaction.verifying_key).unwrap(),
					Account::address(&transaction2.verifying_key).unwrap()
				);
				assert_eq!(transaction.transaction_type, transaction2.transaction_type);
				assert_eq!(transaction.fee_limit, transaction2.fee_limit);
				assert_eq!(transaction.signature, transaction2.signature);
				assert_eq!(transaction.verifying_key, transaction2.verifying_key);
			}
		}
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_remove_expired_transactions() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let expiration_seconds: TimeStamp = 2; // Set expiration time to 2 seconds

		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		// Create a new mempool with expiration time and rate limit
		let mut mempool = Mempool::new(
			100,
			1000,
			10000,
			60000,
			expiration_seconds,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);

		let recipient: Address = [34u8; 20].into();

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();
		let mut account = create_account(sender, &account_state).await;
		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 10;
		let fee_limit = 100;

		let transaction_type_native_tx =
			TransactionTypeNativeTX::NativeTokenTransfer(recipient, amount.to_string());

		let payload = TXNT { nonce, transaction_type: transaction_type_native_tx, fee_limit };

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		assert!(mempool.add_transaction(transaction, &db_pool_conn.clone()).await.is_ok());

		// Wait for the expiration time to pass
		std::thread::sleep(std::time::Duration::from_millis(
			(expiration_seconds + 1).try_into().unwrap(),
		));

		// Call remove_expired_transactions
		mempool.remove_expired_transactions().unwrap().await;

		// Verify that the transaction is removed from the mempool
		assert!(mempool.get_transactions().await.is_empty());
	}

	async fn get_transactions<'a>(account_state: &AccountState<'a>) -> Vec<Transaction> {
		let cluster_address = [0; 20];
		let recipient: Address = [36u8; 20].into();

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();
		let mut account = create_account(sender, &account_state).await;
		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		// Add transactions with different fees
		let nonce1 = get_nonce(&mut account_manager, &account_state).await;
		let nonce2 = get_nonce(&mut account_manager, &account_state).await + 1;
		let nonce3 = get_nonce(&mut account_manager, &account_state).await + 2;
		let transactions = vec![
			Transaction::new(
				nonce1,
				TransactionType::NativeTokenTransfer(recipient, 10),
				50,
				TXNT {
					nonce: nonce1,
					transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
						recipient,
						10.to_string(),
					),
					fee_limit: 50,
				}
				.sign_with_ecdsa(key_space.secret_key)
				.unwrap(),
				key_space.public_key,
			),
			Transaction::new(
				nonce2,
				TransactionType::NativeTokenTransfer(recipient, 20),
				100, //Medium fee
				TXNT {
					nonce: nonce2,
					transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
						recipient,
						20.to_string(),
					),
					fee_limit: 100,
				}
				.sign_with_ecdsa(key_space.secret_key)
				.unwrap(),
				key_space.public_key,
			),
			Transaction::new(
				nonce3,
				TransactionType::NativeTokenTransfer(recipient, 30),
				200, //High fee
				TXNT {
					nonce: nonce3,
					transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
						recipient,
						"30".to_string(),
					),
					fee_limit: 200,
				}
				.sign_with_ecdsa(key_space.secret_key)
				.unwrap(),
				key_space.public_key,
			),
		];
		transactions
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_mempool_transactions_priority() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			3600,
			600,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);
		let transactions = get_transactions(&account_state).await;
		//println!("transactions: {:?}", transactions);
		// Add transactions to the mempool
		for transaction in transactions {
			mempool.add_transaction(transaction, &db_pool_conn).await.unwrap();
		}
		// Retrieve the transactions from the mempool
		let mempool_transactions = mempool.get_transactions_priority().await;

		// Verify the order of transactions in the transactions_priority vector
		assert_eq!(mempool_transactions.len(), 3);
		assert_eq!(mempool_transactions[0].fee_limit, 200); // Highest fee
		assert_eq!(mempool_transactions[1].fee_limit, 100); // Medium fee
		assert_eq!(mempool_transactions[2].fee_limit, 50); // Lowest fee
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_transaction_conflicts() {
		let cluster_address = [0; 20];
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		perform_table_cleanup(cluster_address.clone()).await;
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;

		let mut mempool = Mempool::new(
			100,
			1000,
			10,
			3600,
			600,
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			verifying_key,
			true,
		);
		let recipient: Address = [38u8; 20].into();

		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();
		let mut account = create_account(sender, &account_state).await;
		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 100;
		let fee_limit = 5;
		let signature = TXNT {
			nonce,
			transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
				recipient,
				amount.to_string(),
			),
			fee_limit,
		}
		.sign_with_ecdsa(key_space.secret_key)
		.unwrap();

		let conflicting_tx1 = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		let amount = 200;
		let fee_limit = 10;
		let signature = TXNT {
			nonce,
			transaction_type: TransactionTypeNativeTX::NativeTokenTransfer(
				recipient,
				amount.to_string(),
			),
			fee_limit,
		}
		.sign_with_ecdsa(key_space.secret_key)
		.unwrap();

		let conflicting_tx2 = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);

		// Add the conflicting transactions to the mempool
		mempool.add_transaction(conflicting_tx1.clone(), &db_pool_conn).await.unwrap();
		mempool.add_transaction(conflicting_tx2.clone(), &db_pool_conn).await.unwrap();

		// Check that only one of the conflicting transactions is present in the mempool
		let transactions = mempool.get_transactions_priority().await;
		assert_eq!(transactions.len(), 1);
		assert_eq!(transactions[0].transaction_type, conflicting_tx2.transaction_type);
		assert_eq!(transactions[0].fee_limit, conflicting_tx2.fee_limit);

		let transactions_priority = mempool.get_transactions_priority().await;
		assert_eq!(transactions_priority.len(), 1);
		assert_eq!(transactions_priority[0].transaction_type, conflicting_tx2.transaction_type);
		assert_eq!(transactions_priority[0].fee_limit, conflicting_tx2.fee_limit);
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_propose_block() {
		let cluster_address = [0; 20];
		perform_table_cleanup(cluster_address.clone()).await;
		std::env::set_var("IS_LOCAL", "true");
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		let network_client = initialize_network().await;
		let (event_tx, _) = broadcast::channel(1000);
		let key_space = KeySpace::new();
		let public_key = key_space.public_key;

		let mut mempool = Mempool::new(
			10,   // Max mempool size
			1000, // Gas limit
			5,    // Rate limit
			60,   // Time frame (seconds)
			3600, // Expiration time (seconds)
			network_client,
			event_tx,
			cluster_address,
			[0; 20],
			key_space.secret_key,
			public_key,
			true,
		);
		let ip_address = "127.0.0.1".as_bytes().to_vec();
		let metadata = "Full Node".as_bytes().to_vec();
		let address = Account::address(&public_key.serialize().to_vec().clone()).unwrap();

		let signature =
			PayloadNode { address, ip_address: ip_address.clone(), metadata: metadata.clone() }
				.sign_with_ecdsa(key_space.secret_key)
				.unwrap();

		let node_info = NodeInfo::new(
			address,
			ip_address,
			metadata,
			cluster_address,
			signature.serialize_compact().to_vec(),
			public_key.serialize().to_vec(),
		);

		let node_info_state = NodeInfoState::new(&db_pool_conn).await.unwrap();
		let block_state = BlockState::new(&db_pool_conn).await.unwrap();
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await.unwrap();

		let block_number = 1;
		node_info_state.store_node_info(&node_info.clone()).await.unwrap();
		// Store block proposer
		block_proposer_state
			.store_block_proposer(cluster_address, block_number, node_info.address.clone())
			.await
			.expect("Failed to store block proposers");

		let transactions = get_transactions(&account_state).await;
		// Add transactions to the mempool
		for transaction in transactions {
			mempool.add_transaction(transaction, &db_pool_conn).await.unwrap();
		}

		println!("test node_info.address.clone(): {:?}", hex::encode(node_info.address.clone()));
		let block_proposer =
			BlockProposer { cluster_address, block_number, address: node_info.address.clone() };
		let result = mempool.propose_block(block_proposer, &db_pool_conn).await;

		println!("{:?}", result);

		//result.map_err(|e| println!("{:?}", e));
		assert!(result.is_ok());
	}
}
