#[cfg(test)]
mod tests {
	use crate::{validate_common::ContractValidator, validate_contract_l1xvm::ValidateContract};
	use account::{account_manager::AccountManager, account_state::AccountState};
	use anyhow::{anyhow, Error};
	use db::db::{Database, DbTxConn};
	use l1x_vrf::secp_vrf::KeySpace;
	use primitives::*;
	use secp256k1::{hashes::sha256, Message};
	use serde::{Deserialize, Serialize};
	use std::fs;
	use system::{
		access::AccessType,
		account::{Account, AccountType},
		config::Config,
		contract::ContractType,
		transaction::{Transaction, TransactionType},
	};

	const CONNECTION_STRING: &str = "127.0.0.1:9042";

	#[derive(Serialize, Deserialize)]
	struct SigningPayload {
		pub nonce: Nonce,

		pub transaction_type: TransactionType,
		pub fee_limit: Balance,
	}

	fn get_contract_code() -> Result<ContractCode, Error> {
		// Read the file as bytes into a Vec<u8>
		let file_contents = match fs::read("src/l1x_contract/l1x_contract.o") {
			Ok(f) => f,
			Err(e) => return Err(anyhow!("File read failed - {}", e)),
		};
		Ok(file_contents)
	}

	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	pub async fn truncate_account_table<'a>(account_state: &AccountState<'a>) -> Result<(), Error> {
		let delete_query = "TRUNCATE account;";
		match account_state.raw_query(delete_query).await {
			Ok(_) => Ok(()),
			Err(e) => Err(e.into()), // Convert the error to the appropriate type
		}
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
		account_manager.get_current_nonce(account_state).await.unwrap()
	}

	#[tokio::test]
	#[serial_test::serial]
	async fn test_validate_native_contract_deployment() {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		let account_state = AccountState::new(&db_pool_conn).await.unwrap();
		truncate_account_table(&account_state).await.unwrap();

		let recipient: Address = [12u8; 20].into();

		let key_space = KeySpace::new();

		let verifying_key = key_space.public_key;
		let sender: Address = Account::address(&verifying_key.serialize().to_vec()).unwrap();

		let mut account = create_account(sender, &account_state).await;
		account.balance = 5000;
		account_state.update_balance(&account).await.unwrap();
		let mut account_manager = AccountManager { account };
		let nonce = get_nonce(&mut account_manager, &account_state).await;
		let amount = 10;
		let fee_limit = 100;
		let contract_code = get_contract_code().unwrap();

		let payload = SigningPayload {
			nonce: nonce + 1,
			transaction_type: TransactionType::SmartContractDeployment {
				access_type: AccessType::PRIVATE,
				contract_type: ContractType::L1XVM,
				contract_code: contract_code.clone(),
				salt: "00000000000000000000000000000000".into(),
				deposit: 0,
			},
			fee_limit,
		};

		//let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let json_str = serde_json::to_string(&payload).unwrap();
		let message = Message::from_hashed_data::<sha256::Hash>(&json_str.as_bytes());
		let signature = key_space.secret_key.sign_ecdsa(message);

		let transaction = Transaction::new(
			nonce + 1,
			TransactionType::SmartContractDeployment {
				access_type: AccessType::PRIVATE,
				contract_type: ContractType::L1XVM,
				contract_code: contract_code.clone(),
				salt: "00000000000000000000000000000000".into(),
				deposit: 0,
			},
			fee_limit,
			signature,
			verifying_key,
		);

		let result = ValidateContract
			.validate_contract_deployment(
				&transaction,
				&contract_code.clone(),
				&Account::address(&verifying_key.serialize().to_vec()).unwrap(),
				&db_pool_conn,
			)
			.await;

		//println!("result: {:?}", result);
		assert!(result.is_ok());
	}
}
