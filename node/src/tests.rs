#[cfg(test)]
mod tests {
	use crate::node::*;

	use account::{account_manager::AccountManager, account_state::AccountState};
	use anyhow::Error;
	use primitives::*;

	use db::db::{Database, DbTxConn};
	use l1x_vrf::{common::SecpVRF, secp_vrf::KeySpace};
	use serde::{Deserialize, Serialize};
	use system::{
		account::{Account, AccountType},
		config::Config,
		mempool::ProcessMempool,
		transaction::{Transaction, TransactionType},
	};

	// Helper function to create a new State for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
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

	async fn get_nonce<'a>(
		account_manager: &mut AccountManager,
		account_state: &AccountState<'a>,
	) -> Nonce {
		account_manager.get_current_nonce(account_state).await.unwrap() + 1
	}

	#[derive(Serialize, Deserialize, Debug)]
	struct PayloadTX {
		pub nonce: Nonce,
		pub transaction_type: TransactionType,
		pub fee_limit: Balance,
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn create_full_node() {
		let mut full_node =
			FullNode::new(
				100,
				100_000,
				15,
				100,
				100,
				true,
				false,
				"/ip4/0.0.0.0/tcp/5010",
				Some(
					"6913aeae91daf21a8381b1af75272fe6fae8ec4a21110674815c8f0691e32758".to_string(),
				), // empty bootnode list
				&vec![],
				1u128,
				[
					117, 16, 73, 56, 186, 164, 124, 84, 168, 96, 4, 239, 153, 140, 199, 108, 46,
					97, 98, 137,
				],
				[
					117, 16, 73, 56, 186, 164, 124, 84, 168, 96, 4, 239, 153, 140, 199, 108, 46,
					97, 98, 137,
				], // Genesis
			)
			.await;

		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		let sender: Address = [1u8; 20].into();
		let recipient: Address = [2u8; 20].into();

		let key_space = KeySpace::new();
		let verifying_key = key_space.public_key;
		let sender = Account::address(&verifying_key.serialize().to_vec()).unwrap();

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

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);
		let (response_mempool_tx, _response_mempool_rx) = tokio::sync::oneshot::channel();
		full_node
			.0
			.mempool_tx
			.send(ProcessMempool::AddTransaction(transaction, response_mempool_tx))
			.await
			.unwrap();

		// Handle incoming network
		// loop {
		//     match full_node.event_receiver.recv().await {
		//         Some(event) => match event {
		//             Event::InboundTransaction { transaction } => {
		//                 full_node
		//                     .mempool
		//                     .add_transaction(transaction)
		//                     .await
		//                     .unwrap_or_else(|_| eprintln!("Failed to add transaction to the
		// mempool!"));             } // Add InboundBlock event in future
		//         },
		//         // Command channel closed, thus shutting down the network event loop.
		//         None => return,
		//     }
		// }
	}
}
