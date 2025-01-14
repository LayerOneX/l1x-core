#[cfg(test)]
mod tests {
	use crate::{execute_common::ContractExecutor, execute_contract_evm, execute_contract_l1xvm};
	use account::{account_manager::AccountManager, account_state::AccountState};
	use anyhow::{anyhow, Error};
	use contract::contract_state::ContractState;
	use contract_instance::contract_instance_state::ContractInstanceState;
	use db::db::{Database, DbTxConn};
	use event::event_state::EventState;
	use evm_runtime::{Context, CreateScheme};
	use l1x_vrf::common::{ByteOps};
	use primitive_types::{H160, H256, U256};
	use primitives::*;
	use rand::Rng;
	use serde::{Deserialize, Serialize};
	use sha3::{Digest, Keccak256};
	use std::{fs, sync::Arc};
	use system::{
		access::AccessType,
		account::{Account, AccountType},
		config::Config,
		contract::{Contract, ContractType},
		contract_instance::ContractInstance,
		network::EventBroadcast,
	};

	use tokio::sync::broadcast;

	const L1X_CONTRACT_CODE_PATH: &str = "../validate/src/l1x_contract/l1x_contract.o";
	const L1X_TEST_CONTRACT_CODE_PATH: &str = "../validate/src/l1x_contract/l1x_test_contract.o";
	const X_TALK_CONTRACT_CODE_PATH: &str =
		"../validate/src/xtalk_contract/xtalk_nft_ad_flow_contract.o";
	const EVM_CREATION_ERC20_CONTRACT_CODE_PATH: &str =
		"../validate/src/evm_contract/creationCode20.txt";
	const EVM_RUNTIME_ERC20_CONTRACT_CODE_PATH: &str =
		"../validate/src/evm_contract/runtimeBytecode20.txt";

	fn get_contract_code(path: &str) -> Result<ContractCode, Error> {
		// Read the file as bytes into a Vec<u8>
		let file_contents = match fs::read(path) {
			Ok(f) => f,
			Err(e) => return Err(anyhow!("File read failed - {}", e)),
		};
		Ok(file_contents)
	}

	// Helper function to create a new State for testing
	async fn database_conn<'a>() -> Result<(DbTxConn<'a>, Config), Error> {
		println!("database_conn");
		let config_data = Config::default();
		Database::new_test(&config_data).await;
		let db_pool_conn = Database::get_test_connection().await.unwrap();
		Ok((db_pool_conn, config_data))
	}

	pub async fn truncate_account_table<'a>(account_state: &AccountState<'a>) {
		account_state.raw_query("TRUNCATE account;").await;
	}

	pub async fn truncate_contract_table<'a>(state: &ContractState<'a>) {
		state.raw_query("TRUNCATE contract;").await;
	}

	pub async fn truncate_event_table<'a>(state: &EventState<'a>) {
		state.raw_query("TRUNCATE event;").await;
	}
	pub async fn truncate_contract_instance_table<'a>(state: &ContractInstanceState<'a>) {
		state.raw_query("TRUNCATE contract_instance_contract_code_map;").await;
		state.raw_query("TRUNCATE contract_instance;").await;
	}

	async fn create_account<'a>(address: Address, account_state: &AccountState<'a>) -> Account {
		let mut account = Account::new(address);
		account_state.create_account(&account).await.unwrap();
		account.balance = 9_000_000_000;
		account_state.update_balance(&account).await.unwrap();
		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		assert_eq!(loaded_account.nonce, account.nonce);
		assert_eq!(loaded_account.account_type, AccountType::User);
		account
	}
	/*async fn create_account<'a>(address: Address, account_state: &AccountState<'a>) -> Account {
		let mut account = Account::new(address);
		account.balance = 9_000_000_000;

		account_state.update_balance(&account).await.unwrap();

		let loaded_account = account_state.get_account(&address).await.unwrap();
		assert_eq!(loaded_account.balance, account.balance);
		//assert_eq!(loaded_account.nonce, account.nonce);
		assert_eq!(loaded_account.account_type, AccountType::User);
		account
	}*/

	async fn get_nonce<'a>(
		account_manager: &mut AccountManager,
		account_state: &AccountState<'a>,
	) -> Nonce {
		account_manager.get_current_nonce(account_state).await.unwrap()
	}

	pub fn generate_salt() -> H256 {
		let mut rng = rand::thread_rng();
		let mut salt: H256 = H256::default();

		for byte in salt.0.iter_mut() {
			*byte = rng.gen();
		}
		salt
	}

	async fn deploy_and_init_contract<'a>(
		account_address: &Address,
		scheme: CreateScheme,
		context: Context,
		cluster_address: &Address,
		account_manager: &mut AccountManager,
		account_state: &AccountState<'a>,
		contract_state: &ContractState<'a>,
		contract_instance_state: &ContractInstanceState<'a>,
		event_state: &EventState<'a>,
		arguments: ContractArgument,
		contract_code: &ContractCode,
		transaction_hash: &TransactionHash,
		nonce: Nonce,
		event_tx: broadcast::Sender<EventBroadcast>,
	) -> (Contract, ContractInstance) {
		let (db_pool_conn, config) = database_conn().await.unwrap();
		// Initialize test data
		let result = execute_contract_l1xvm::ExecuteContract {}
			.execute_contract_deployment(
				account_address,
				scheme.clone(),
				context.clone(),
				cluster_address,
				AccessType::PUBLIC as i8,
				&contract_code,
				nonce + 1,
				&[10u8; 32],
				1,
				&[0u8; 32],
				0,
				Arc::new(&db_pool_conn),
				event_tx.clone(),
			)
			.await;
		println!("execute_native_contract_deployment- {:?}", result);
		assert!(result.is_ok());
		let contract_address = result.unwrap();
		let contract = Contract {
			address: contract_address,
			access: AccessType::PUBLIC as i8,
			code: contract_code.clone(),
			owner_address: *account_address,
			r#type: 0,
		};
		let result = execute_contract_l1xvm::ExecuteContract {}
			.execute_contract_init(
				account_address,
				context,
				Gas::MAX,
				cluster_address,
				&contract,
				arguments.clone(),
				nonce + 2,
				&[11u8; 32],
				1,
				&[0u8; 32],
				0,
				Arc::new(&db_pool_conn),
				event_tx,
			)
			.await;
		println!(
			"execute_native_contract_init- {:?}, arguments: {}",
			result,
			String::from_utf8_lossy(&arguments)
		);
		assert!(result.is_ok());
		let contract_instance = result.unwrap().0;
		(contract, contract_instance)
	}

	mod execute_native_contract {
		use std::sync::Arc;

		use vm_execution_fee::execution_fees::READONLY_CALL_DEFAULT_GAS_LIMIT;

		use super::*;
		#[tokio::test(flavor = "current_thread")]
		async fn test_execute_native_contract() {
			let (db_pool_conn, config) = database_conn().await.unwrap();
			//let event_state = EventState::new(&db_pool_conn).await.unwrap();
			let account_address: Address = [12u8; 20].into();
			let cluster_address: Address = [22u8; 20].into();
			let transaction_hash: TransactionHash = [01u8; 32].into();
			let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
			truncate_contract_table(&contract_state).await;

			let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
			truncate_contract_instance_table(&contract_instance_state).await;

			let account_state = AccountState::new(&db_pool_conn).await.unwrap();
			truncate_account_table(&account_state).await;

			let event_state = EventState::new(&db_pool_conn).await.unwrap();
			truncate_event_table(&event_state).await;

			let account = create_account(account_address, &account_state).await;

			let mut account_manager = AccountManager { account };
			let nonce = get_nonce(&mut account_manager, &account_state).await;
			let contract_code = get_contract_code(L1X_CONTRACT_CODE_PATH).unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let (event_tx, _) = broadcast::channel(1000);

			let (contract, contract_instance) = deploy_and_init_contract(
				&account_address,
				scheme,
				context.clone(),
				&cluster_address,
				&mut account_manager,
				&account_state,
				&contract_state,
				&contract_instance_state,
				&event_state,
				"{}".to_string().into_bytes(),
				&contract_code,
				&transaction_hash,
				nonce,
				event_tx.clone(),
			)
			.await;

			println!(
				"contract_instance_addr: {:?}",
				hex::encode(contract_instance.instance_address)
			);
			let function: ContractFunction = "add_name".to_string().into_bytes();
			let arguments: ContractArgument = r#"{"name": "Suman"}"#.to_string().into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			println!("execute_native_contract_function_call- {:?}", result);
			assert!(result.is_ok());

			let function: ContractFunction = "get_names".to_string().into_bytes();
			let arguments: ContractArgument = r#"{}"#.to_string().into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					true,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;
			assert!(result.is_ok());

			let function: ContractFunction = "hello".to_string().into_bytes();
			let arguments: ContractArgument = r#"{}"#.to_string().into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					true,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;
			assert!(result.is_ok());
		}

		#[tokio::test(flavor = "current_thread")]
		async fn test_execute_native_contract_execution_fee() {
			let (db_pool_conn, config) = database_conn().await.unwrap();
			let account_address: Address = [12u8; 20].into();
			let cluster_address: Address = [22u8; 20].into();
			let transaction_hash: TransactionHash = [01u8; 32].into();

			let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
			truncate_contract_table(&contract_state).await;

			let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
			truncate_contract_instance_table(&contract_instance_state).await;

			let account_state = AccountState::new(&db_pool_conn).await.unwrap();
			truncate_account_table(&account_state).await;

			let event_state = EventState::new(&db_pool_conn).await.unwrap();
			truncate_event_table(&event_state).await;

			let account = create_account(account_address, &account_state).await;
			let mut account_manager = AccountManager { account };
			let nonce = get_nonce(&mut account_manager, &account_state).await;
			let contract_code = get_contract_code(L1X_TEST_CONTRACT_CODE_PATH).unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let (event_tx, _) = broadcast::channel(1000);

			let (contract, contract_instance) = deploy_and_init_contract(
				&account_address,
				scheme,
				context.clone(),
				&cluster_address,
				&mut account_manager,
				&account_state,
				&contract_state,
				&contract_instance_state,
				&event_state,
				"{}".to_string().into_bytes(),
				&contract_code,
				&transaction_hash,
				nonce,
				event_tx.clone(),
			)
			.await;

			let gas_limit: Gas = 10000;
			let function: ContractFunction = "inc_counter".to_string().into_bytes();
			let arguments: ContractArgument = r#"{}"#.to_string().into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			println!("test_execute_native_contract_execution_fee - result: {:?}", result);
			assert!(result.is_ok());

			let gas_limit: Gas = 1;
			let function: ContractFunction = "inc_counter".to_string().into_bytes();
			let arguments: ContractArgument = r#"{}"#.to_string().into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			println!("mm result: {:?}", result);
			//assert!(result.is_err(), "Must fail with 'Out of Gas' error");

			let gas_limit: Gas = READONLY_CALL_DEFAULT_GAS_LIMIT;
			let function: ContractFunction = "get_counter".to_string().into_bytes();
			let arguments: ContractArgument = r#"{}"#.to_string().into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					true,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			assert!(result.is_ok(), "Must pass because this is not a huge method");
			assert_eq!(result.unwrap().burnt_gas, 0);

			// Read-only method calls another read-only method
			let gas_limit: Gas = READONLY_CALL_DEFAULT_GAS_LIMIT;
			let function: ContractFunction = "cross_contract_call".to_string().into_bytes();
			let arguments: ContractArgument = format!(
				"{{
				\"address\": \"{}\",
				\"method_name\":\"get_counter\",
				\"args\": [123, 125],
				\"read_only\": true,
				\"gas_limit\": \"10000\"
			}}",
				hex::encode(&contract_instance.instance_address)
			)
			.into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					true,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			println!("result: {:?}", result);
			assert!(result.is_ok());
			assert_eq!(result.unwrap().burnt_gas, 0);

			// Mutable method calls a read-only method
			let gas_limit = 60000;
			let function: ContractFunction = "cross_contract_call".to_string().into_bytes();
			let arguments: ContractArgument = format!(
				"{{
				\"address\": \"{}\",
				\"method_name\":\"get_counter\",
				\"args\": [123, 125],
				\"read_only\": true,
				\"gas_limit\": \"10000\"
			}}",
				hex::encode(&contract_instance.instance_address)
			)
			.into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			assert!(result.is_ok());

			// Mutable method calls another mutable method
			let gas_limit = Gas::MAX;
			let function: ContractFunction = "cross_contract_call".to_string().into_bytes();
			let arguments: ContractArgument = format!(
				"{{
					\"address\": \"{}\",
					\"method_name\":\"inc_counter\",
					\"args\": [123, 125],
					\"read_only\": false,
					\"gas_limit\": \"10000\"
				}}",
				hex::encode(&contract_instance.instance_address),
			)
			.into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			//println!("{}", String::from_utf8_lossy(&arguments));
			//println!("resutlt: {:?}", result);
			assert!(result.is_ok());
			assert_ne!(result.unwrap().burnt_gas, 0);

			// Mutable method calls another mutable method
			// Test fails because there is not enough gas for the cross-contract call
			let gas_limit = 50000;
			let function: ContractFunction = "cross_contract_call".to_string().into_bytes();
			let arguments: ContractArgument = format!(
				"{{
					\"address\": \"{}\",
					\"method_name\":\"inc_counter\",
					\"args\": [123, 125],
					\"read_only\": false,
					\"gas_limit\": \"10000\"
				}}",
				hex::encode(&contract_instance.instance_address),
			)
			.into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			// println!("{}", String::from_utf8_lossy(&arguments));
			assert!(result.is_err(), "Must fail with Out of Gas");

			// Mutable method calls another mutable method
			// Test fails because there is not enough gas for the called contract
			let gas_limit = Gas::MAX;
			let function: ContractFunction = "cross_contract_call".to_string().into_bytes();
			let arguments: ContractArgument = format!(
				"{{
				\"address\": \"{}\",
				\"method_name\":\"inc_counter\",
				\"args\": [123, 125],
				\"read_only\": false,
				\"gas_limit\": \"500\"
			}}",
				hex::encode(&contract_instance.instance_address),
			)
			.into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			assert!(result.is_err(), "Must fail with Out of Gas error");

			let gas_limit = 100_000;
			let function: ContractFunction = "infinite_loop".to_string().into_bytes();
			let arguments: ContractArgument = "{}".to_string().into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					gas_limit,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			assert!(result.is_err(), "Must fail with Out of Gas error");
		}
	}

	mod x_talk_nft_ad_flow {
		use super::*;
		use system::{contract::Contract, contract_instance::ContractInstance};

		#[derive(Serialize, Deserialize, Default)]
		pub struct AdvertisementStarted {
			nft_contract: String,
			token_id: u128,
			token_uri: String,
			owner: String,
			price: u128,
		}

		#[derive(Serialize, Deserialize, Default)]
		pub struct AdvertisementTransferred {
			global_tx_id: String,
			to: String,
		}

		#[derive(Serialize, Deserialize, Default)]
		pub struct AdvertisementFinished {
			global_tx_id: String,
			nft_contract: String,
			token_id: String,
			old_owner: String,
			new_owner: String,
			price: u128,
		}
		#[ignore]
		#[tokio::test(flavor = "current_thread")]
		async fn test_x_talk_nft_ad_flow_native_contract() {
			let account_address: Address = [12u8; 20].into();
			let cluster_address: Address = [22u8; 20].into();
			let transaction_hash: TransactionHash = [02u8; 32].into();

			let (db_pool_conn, config) = database_conn().await.unwrap();
			let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
			truncate_contract_table(&contract_state).await;

			let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
			truncate_contract_instance_table(&contract_instance_state).await;

			let account_state = AccountState::new(&db_pool_conn).await.unwrap();
			truncate_account_table(&account_state).await;

			let event_state = EventState::new(&db_pool_conn).await.unwrap();
			truncate_event_table(&event_state).await;

			let account = create_account(account_address, &account_state).await;
			let mut account_manager = AccountManager { account };
			let nonce = get_nonce(&mut account_manager, &account_state).await;
			let contract_code = get_contract_code(X_TALK_CONTRACT_CODE_PATH).unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let (event_tx, _) = broadcast::channel(1000);

			let (contract, contract_instance) = deploy_and_init_contract(
				&account_address,
				scheme,
				context.clone(),
				&cluster_address,
				&mut account_manager,
				&account_state,
				&contract_state,
				&contract_instance_state,
				&event_state,
				vec![],
				&contract_code,
				&transaction_hash,
				nonce,
				event_tx.clone(),
			)
			.await;

			let function: ContractFunction = "save_event_data".to_string().into_bytes();

			let tx_advertisement_started = AdvertisementStarted {
				nft_contract: "0x123".to_string(),
				token_id: 123,
				token_uri: "https://www.google.com".to_string(),
				owner: "0x123".to_string(),
				price: 123,
			};

			let tx = tx_advertisement_started.to_bytes().unwrap();

			let tx_str = format!(
				"[{}]",
				tx.iter().map(|num| num.to_string()).collect::<Vec<String>>().join(", ")
			);

			let obj: AdvertisementStarted = bincode::deserialize(tx.as_slice()).unwrap();

			let arguments: ContractArgument = format!(
				r#"{{"global_tx_id": "tx11", "event_type": "AdvertisementStarted", "event_data": {}}}"#,
				tx_str
			)
			.to_string()
			.into_bytes();

			/*
			Save function passes args
			This works
			let arguments: ContractArgument = format!(
				r#"{{"global_tx_id": "tx11", "event_type": "mint", "event_data": {}}}"#,
				tx_str
			)
			*/
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			println!("execute_native_contract_function_call- {:?}", result);
			assert!(result.is_ok());

			let function: ContractFunction = "update_state".to_string().into_bytes();
			let arguments: ContractArgument =
				r#"{"global_tx_id": "tx11"}"#.to_string().into_bytes();

			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			println!("UPDATE STATE - {:?}", result);
			assert!(result.is_ok());

			let function: ContractFunction = "get_payload_to_sign".to_string().into_bytes();
			let arguments: ContractArgument =
				r#"{"global_tx_id": "tx11"}"#.to_string().into_bytes();

			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;
			println!("GET PAYLOAD TO SIGN - {:?}", result);
			assert!(result.is_ok());

			// This works
			let function: ContractFunction = "total_events".to_string().into_bytes();
			let arguments: ContractArgument = r#"{}"#.to_string().into_bytes();
			let result = execute_contract_l1xvm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;
			println!("total_events- {:?}", result);
			assert!(result.is_ok());
		}
	}

	mod execute_evm_contract {
		use super::*;
		#[ignore]
		#[tokio::test(flavor = "current_thread")]
		async fn test_execute_evm_contract() {
			let account_address: Address = [12u8; 20].into();
			let cluster_address: Address = [22u8; 20].into();
			let transaction_hash: TransactionHash = [01u8; 32].into();
			let (db_pool_conn, config) = database_conn().await.unwrap();
			let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
			truncate_contract_table(&contract_state).await;
			let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
			truncate_contract_instance_table(&contract_instance_state).await;

			let account_state = AccountState::new(&db_pool_conn).await.unwrap();
			truncate_account_table(&account_state).await;

			let event_state = EventState::new(&db_pool_conn).await.unwrap();
			truncate_event_table(&event_state).await;
			let account = create_account(account_address, &account_state).await;
			let mut account_manager = AccountManager { account };
			let nonce = get_nonce(&mut account_manager, &account_state).await;
			let contract_code: ContractCode = hex::decode("608060405234801561001057600080fd5b50610191806100206000396000f3fe608060405234801561001057600080fd5b506004361061004c5760003560e01c806319ff1d211461004e5780632a1afcd91461008357806360fe47b11461009a5780636d4ce63c146100ad575b005b604080518082018252600a8152691a195b1b1bc81ddbdc9960b21b6020820152905161007a91906100cd565b60405180910390f35b61008c60005481565b60405190815260200161007a565b61008c6100a836600461011b565b6100b5565b60005461008c565b60006100c2826001610134565b600081905592915050565b600060208083528351808285015260005b818110156100fa578581018301518582016040015282016100de565b506000604082860101526040601f19601f8301168501019250505092915050565b60006020828403121561012d57600080fd5b5035919050565b8082018082111561015557634e487b7160e01b600052601160045260246000fd5b9291505056fea26469706673582212205dd1d243106225101604ee0ad31d85e42587a9d6a230f90b71bea6cc9038b8eb64736f6c63430008150033").unwrap();

			let function: ContractFunction = "".into(); // function name is not needed as parameter, it is included in argument byte codes
			let arguments: ContractArgument = hex::decode(
				"60fe47b1000000000000000000000000000000000000000000000000000000000000007b",
			)
			.unwrap();
			let contract_instance = ContractInstance::default();
			let contract = Contract::new(
				[0u8; 20],
				AccessType::PUBLIC as i8,
				ContractType::EVM as i8,
				contract_code.clone(),
				account_address,
			);
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let (event_tx, _) = broadcast::channel(1000);

			let result = execute_contract_evm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			assert_eq!(
				"0000000000000000000000000000000000000000000000000000000000000037",
				hex::encode(result.unwrap().result)
			);

			let function: ContractFunction = "".into(); // function name is not needed as parameter, it is included in argument byte codes
			let arguments: ContractArgument = hex::decode("19ff1d21").unwrap();
			let result = execute_contract_evm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context.clone(),
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&function,
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;
			assert_eq!(
				"00000000000000000000000000000000000000000000000000000000000003db",
				hex::encode(result.unwrap().result)
			);
		}
	}

	mod execute_evm_contract_storage {
		use std::sync::Arc;

		use super::*;

		#[ignore]
		#[tokio::test(flavor = "current_thread")]
		async fn test_execute_evm_contract() {
			let account_address: Address = [12u8; 20].into();
			let cluster_address: Address = [22u8; 20].into();
			let transaction_hash: TransactionHash = [01u8; 32].into();

			let (db_pool_conn, config) = database_conn().await.unwrap();
			let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
			truncate_contract_table(&contract_state).await;
			let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
			truncate_contract_instance_table(&contract_instance_state).await;
			let account_state = AccountState::new(&db_pool_conn).await.unwrap();
			truncate_account_table(&account_state).await;
			let event_state = EventState::new(&db_pool_conn).await.unwrap();
			truncate_event_table(&event_state).await;

			let account = create_account(account_address, &account_state).await;
			let mut account_manager = AccountManager { account };
			let nonce = get_nonce(&mut account_manager, &account_state).await;
			let contract_code: ContractCode = hex::decode("608060405234801561001057600080fd5b506064600055610191806100256000396000f3fe608060405234801561001057600080fd5b506004361061004c5760003560e01c806319ff1d211461004e5780632a1afcd91461008357806360fe47b11461009a5780636d4ce63c146100ad575b005b604080518082018252600a8152691a195b1b1bc81ddbdc9960b21b6020820152905161007a91906100cd565b60405180910390f35b61008c60005481565b60405190815260200161007a565b61008c6100a836600461011b565b6100b5565b60005461008c565b60006100c2826001610134565b600081905592915050565b600060208083528351808285015260005b818110156100fa578581018301518582016040015282016100de565b506000604082860101526040601f19601f8301168501019250505092915050565b60006020828403121561012d57600080fd5b5035919050565b8082018082111561015557634e487b7160e01b600052601160045260246000fd5b9291505056fea2646970667358221220bcbfbdf68dabe2f6b4310a95a9b652a1ccbf7d58beeb25f975de06832323d83664736f6c63430008150033").unwrap();
			let arguments: ContractArgument = hex::decode("").unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let (event_tx, _) = broadcast::channel(1000);

			let result = execute_contract_evm::ExecuteContract {}
				.execute_contract_deployment(
					&account_address,
					scheme,
					context,
					&cluster_address,
					AccessType::PUBLIC as i8,
					&contract_code,
					nonce,
					&transaction_hash,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			assert!(result.is_ok());
		}

		#[ignore]
		#[tokio::test(flavor = "current_thread")]
		async fn test_execute_evm_contract_multi() {
			let account_address: Address = [12u8; 20].into();
			let cluster_address: Address = [22u8; 20].into();
			let transaction_hash: TransactionHash = [01u8; 32].into();
			let (db_pool_conn, config) = database_conn().await.unwrap();

			let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
			truncate_contract_table(&contract_state).await;
			let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
			truncate_contract_instance_table(&contract_instance_state).await;
			let account_state = AccountState::new(&db_pool_conn).await.unwrap();
			truncate_account_table(&account_state).await;
			let event_state = EventState::new(&db_pool_conn).await.unwrap();
			truncate_event_table(&event_state).await;

			let account = create_account(account_address, &account_state).await;
			let mut account_manager = AccountManager { account };
			let nonce = get_nonce(&mut account_manager, &account_state).await;
			let contract_code: ContractCode = hex::decode("608060405234801561001057600080fd5b50606460005560408051808201909152600581526468656c6c6f60d81b602082015260019061003f90826100e4565b506101a3565b634e487b7160e01b600052604160045260246000fd5b600181811c9082168061006f57607f821691505b60208210810361008f57634e487b7160e01b600052602260045260246000fd5b50919050565b601f8211156100df57600081815260208120601f850160051c810160208610156100bc5750805b601f850160051c820191505b818110156100db578281556001016100c8565b5050505b505050565b81516001600160401b038111156100fd576100fd610045565b6101118161010b845461005b565b84610095565b602080601f831160018114610146576000841561012e5750858301515b600019600386901b1c1916600185901b1785556100db565b600085815260208120601f198616915b8281101561017557888601518255948401946001909101908401610156565b50858210156101935787850151600019600388901b60f8161c191681555b5050505050600190811b01905550565b61026f806101b26000396000f3fe608060405234801561001057600080fd5b50600436106100575760003560e01c806319ff1d21146100595780631d24b7e9146100915780632a1afcd91461009957806360fe47b1146100b05780636d4ce63c146100c3575b005b60408051808201909152600a8152691a195b1b1bc81ddbdc9960b21b60208201525b6040516100889190610171565b60405180910390f35b61007b6100cb565b6100a260005481565b604051908152602001610088565b6100a26100be3660046101bf565b610159565b6000546100a2565b600180546100d8906101d8565b80601f0160208091040260200160405190810160405280929190818152602001828054610104906101d8565b80156101515780601f1061012657610100808354040283529160200191610151565b820191906000526020600020905b81548152906001019060200180831161013457829003601f168201915b505050505081565b6000610166826001610212565b600081905592915050565b600060208083528351808285015260005b8181101561019e57858101830151858201604001528201610182565b506000604082860101526040601f19601f8301168501019250505092915050565b6000602082840312156101d157600080fd5b5035919050565b600181811c908216806101ec57607f821691505b60208210810361020c57634e487b7160e01b600052602260045260246000fd5b50919050565b8082018082111561023357634e487b7160e01b600052601160045260246000fd5b9291505056fea2646970667358221220b66035851a9468064cb901487ad844ba8ae2ea76fa1ccc46313ab6f6571bb3b364736f6c63430008150033").unwrap();
			let arguments: ContractArgument = hex::decode("").unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let (event_tx, _) = broadcast::channel(1000);

			let result = execute_contract_evm::ExecuteContract {}
				.execute_contract_deployment(
					&account_address,
					scheme,
					context,
					&cluster_address,
					AccessType::PUBLIC as i8,
					&contract_code,
					nonce,
					&transaction_hash,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			assert!(result.is_ok());
		}

		#[ignore]
		#[tokio::test(flavor = "current_thread")]
		async fn test_execute_evm_contract_input_from_constructor() {
			let account_address: Address = [12u8; 20].into();
			let cluster_address: Address = [22u8; 20].into();
			let transaction_hash: TransactionHash = [01u8; 32].into();

			let (db_pool_conn, config) = database_conn().await.unwrap();
			let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
			truncate_contract_table(&contract_state).await;
			let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
			truncate_contract_instance_table(&contract_instance_state).await;
			let account_state = AccountState::new(&db_pool_conn).await.unwrap();
			truncate_account_table(&account_state).await;
			let event_state = EventState::new(&db_pool_conn).await.unwrap();
			truncate_event_table(&event_state).await;

			let account = create_account(account_address, &account_state).await;
			let mut account_manager = AccountManager { account };
			let nonce = get_nonce(&mut account_manager, &account_state).await;
			let contract_code: ContractCode = hex::decode("608060405234801561001057600080fd5b5060405161057138038061057183398101604081905261002f91610073565b6000829055600161004082826101d0565b505070100000000000000000000000000000000a6002555061028f565b634e487b7160e01b600052604160045260246000fd5b6000806040838503121561008657600080fd5b8251602080850151919350906001600160401b03808211156100a757600080fd5b818601915086601f8301126100bb57600080fd5b8151818111156100cd576100cd61005d565b604051601f8201601f19908116603f011681019083821181831017156100f5576100f561005d565b81604052828152898684870101111561010d57600080fd5b600093505b8284101561012f5784840186015181850187015292850192610112565b60008684830101528096505050505050509250929050565b600181811c9082168061015b57607f821691505b60208210810361017b57634e487b7160e01b600052602260045260246000fd5b50919050565b601f8211156101cb57600081815260208120601f850160051c810160208610156101a85750805b601f850160051c820191505b818110156101c7578281556001016101b4565b5050505b505050565b81516001600160401b038111156101e9576101e961005d565b6101fd816101f78454610147565b84610181565b602080601f831160018114610232576000841561021a5750858301515b600019600386901b1c1916600185901b1785556101c7565b600085815260208120601f198616915b8281101561026157888601518255948401946001909101908401610242565b508582101561027f5787850151600019600388901b60f8161c191681555b5050505050600190811b01905550565b6102d38061029e6000396000f3fe608060405234801561001057600080fd5b506004361061007a5760003560e01c80632a1afcd9116100585780632a1afcd9146100e357806360fe47b1146100fa5780636d4ce63c1461010d578063a56dfe4a1461011557005b80630c55699c1461007c57806319ff1d21146100ac5780631d24b7e9146100db575b005b60025461008f906001600160801b031681565b6040516001600160801b0390911681526020015b60405180910390f35b60408051808201909152600a8152691a195b1b1bc81ddbdc9960b21b60208201525b6040516100a391906101d5565b6100ce61012f565b6100ec60005481565b6040519081526020016100a3565b6100ec610108366004610223565b6101bd565b6000546100ec565b60025461008f90600160801b90046001600160801b031681565b6001805461013c9061023c565b80601f01602080910402602001604051908101604052809291908181526020018280546101689061023c565b80156101b55780601f1061018a576101008083540402835291602001916101b5565b820191906000526020600020905b81548152906001019060200180831161019857829003601f168201915b505050505081565b60006101ca826001610276565b600081905592915050565b600060208083528351808285015260005b81811015610202578581018301518582016040015282016101e6565b506000604082860101526040601f19601f8301168501019250505092915050565b60006020828403121561023557600080fd5b5035919050565b600181811c9082168061025057607f821691505b60208210810361027057634e487b7160e01b600052602260045260246000fd5b50919050565b8082018082111561029757634e487b7160e01b600052601160045260246000fd5b9291505056fea2646970667358221220eac53c199b35040f839e7def99739ad02a652e03e7e3cae5a7e3ec97c3446ac164736f6c634300081500330000000000000000000000000000000000000000000000000000000000000064000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000026869000000000000000000000000000000000000000000000000000000000000").unwrap();
			let arguments: ContractArgument = hex::decode("").unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let (event_tx, _) = broadcast::channel(1000);

			let result = execute_contract_evm::ExecuteContract {}
				.execute_contract_deployment(
					&account_address,
					scheme,
					context,
					&cluster_address,
					AccessType::PUBLIC as i8,
					&contract_code,
					nonce,
					&transaction_hash,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			let contract_instance_address = result.unwrap();

			let contract_instance_address: [u8; 20] = {
				let mut array = [0u8; 20];
				array.copy_from_slice(&contract_instance_address);
				array
			};
			let contract_instance = contract_instance_state
				.get_contract_instance(&contract_instance_address)
				.await
				.unwrap();
			let contract =
				contract_state.get_contract(&contract_instance.contract_address).await.unwrap();

			let arguments: ContractArgument = hex::decode("").unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let result = execute_contract_evm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context,
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&vec![],
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			assert_eq!(
                "608060405234801561001057600080fd5b506004361061007a5760003560e01c80632a1afcd9116100585780632a1afcd9146100e357806360fe47b1146100fa5780636d4ce63c1461010d578063a56dfe4a1461011557005b80630c55699c1461007c57806319ff1d21146100ac5780631d24b7e9146100db575b005b60025461008f906001600160801b031681565b6040516001600160801b0390911681526020015b60405180910390f35b60408051808201909152600a8152691a195b1b1bc81ddbdc9960b21b60208201525b6040516100a391906101d5565b6100ce61012f565b6100ec60005481565b6040519081526020016100a3565b6100ec610108366004610223565b6101bd565b6000546100ec565b60025461008f90600160801b90046001600160801b031681565b6001805461013c9061023c565b80601f01602080910402602001604051908101604052809291908181526020018280546101689061023c565b80156101b55780601f1061018a576101008083540402835291602001916101b5565b820191906000526020600020905b81548152906001019060200180831161019857829003601f168201915b505050505081565b60006101ca826001610276565b600081905592915050565b600060208083528351808285015260005b81811015610202578581018301518582016040015282016101e6565b506000604082860101526040601f19601f8301168501019250505092915050565b60006020828403121561023557600080fd5b5035919050565b600181811c9082168061025057607f821691505b60208210810361027057634e487b7160e01b600052602260045260246000fd5b50919050565b8082018082111561029757634e487b7160e01b600052601160045260246000fd5b9291505056fea2646970667358221220eac53c199b35040f839e7def99739ad02a652e03e7e3cae5a7e3ec97c3446ac164736f6c63430008150033", //contract address, use it for function call
                hex::encode(result.unwrap().result)
            );
		}

		#[tokio::test(flavor = "current_thread")]
		async fn test_execute_evm_contract_er20_token() {
			let account_address: Address = [12u8; 20].into();
			let cluster_address: Address = [22u8; 20].into();
			let transaction_hash: TransactionHash = [01u8; 32].into();

			let (db_pool_conn, config) = database_conn().await.unwrap();
			let contract_state = ContractState::new(&db_pool_conn).await.unwrap();
			truncate_contract_table(&contract_state).await;
			let contract_instance_state = ContractInstanceState::new(&db_pool_conn).await.unwrap();
			truncate_contract_instance_table(&contract_instance_state).await;
			let account_state = AccountState::new(&db_pool_conn).await.unwrap();
			truncate_account_table(&account_state).await;
			let event_state = EventState::new(&db_pool_conn).await.unwrap();
			truncate_event_table(&event_state).await;

			let account = create_account(account_address, &account_state).await;
			let mut account_manager = AccountManager { account };
			let nonce = get_nonce(&mut account_manager, &account_state).await;
			//let contract_code: ContractCode =
			// hex::decode("
			// 608060405234801561001057600080fd5b5060405161057138038061057183398101604081905261002f91610073565b6000829055600161004082826101d0565b505070100000000000000000000000000000000a6002555061028f565b634e487b7160e01b600052604160045260246000fd5b6000806040838503121561008657600080fd5b8251602080850151919350906001600160401b03808211156100a757600080fd5b818601915086601f8301126100bb57600080fd5b8151818111156100cd576100cd61005d565b604051601f8201601f19908116603f011681019083821181831017156100f5576100f561005d565b81604052828152898684870101111561010d57600080fd5b600093505b8284101561012f5784840186015181850187015292850192610112565b60008684830101528096505050505050509250929050565b600181811c9082168061015b57607f821691505b60208210810361017b57634e487b7160e01b600052602260045260246000fd5b50919050565b601f8211156101cb57600081815260208120601f850160051c810160208610156101a85750805b601f850160051c820191505b818110156101c7578281556001016101b4565b5050505b505050565b81516001600160401b038111156101e9576101e961005d565b6101fd816101f78454610147565b84610181565b602080601f831160018114610232576000841561021a5750858301515b600019600386901b1c1916600185901b1785556101c7565b600085815260208120601f198616915b8281101561026157888601518255948401946001909101908401610242565b508582101561027f5787850151600019600388901b60f8161c191681555b5050505050600190811b01905550565b6102d38061029e6000396000f3fe608060405234801561001057600080fd5b506004361061007a5760003560e01c80632a1afcd9116100585780632a1afcd9146100e357806360fe47b1146100fa5780636d4ce63c1461010d578063a56dfe4a1461011557005b80630c55699c1461007c57806319ff1d21146100ac5780631d24b7e9146100db575b005b60025461008f906001600160801b031681565b6040516001600160801b0390911681526020015b60405180910390f35b60408051808201909152600a8152691a195b1b1bc81ddbdc9960b21b60208201525b6040516100a391906101d5565b6100ce61012f565b6100ec60005481565b6040519081526020016100a3565b6100ec610108366004610223565b6101bd565b6000546100ec565b60025461008f90600160801b90046001600160801b031681565b6001805461013c9061023c565b80601f01602080910402602001604051908101604052809291908181526020018280546101689061023c565b80156101b55780601f1061018a576101008083540402835291602001916101b5565b820191906000526020600020905b81548152906001019060200180831161019857829003601f168201915b505050505081565b60006101ca826001610276565b600081905592915050565b600060208083528351808285015260005b81811015610202578581018301518582016040015282016101e6565b506000604082860101526040601f19601f8301168501019250505092915050565b60006020828403121561023557600080fd5b5035919050565b600181811c9082168061025057607f821691505b60208210810361027057634e487b7160e01b600052602260045260246000fd5b50919050565b8082018082111561029757634e487b7160e01b600052601160045260246000fd5b9291505056fea2646970667358221220eac53c199b35040f839e7def99739ad02a652e03e7e3cae5a7e3ec97c3446ac164736f6c634300081500330000000000000000000000000000000000000000000000000000000000000064000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000026869000000000000000000000000000000000000000000000000000000000000"
			// ).unwrap();
			let contract_code: ContractCode =
				hex::decode(get_contract_code(EVM_CREATION_ERC20_CONTRACT_CODE_PATH).unwrap())
					.unwrap();
			let arguments: ContractArgument = hex::decode("").unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let (event_tx, _) = broadcast::channel(1000);

			let result = execute_contract_evm::ExecuteContract {}
				.execute_contract_deployment(
					&account_address,
					scheme,
					context,
					&cluster_address,
					AccessType::PUBLIC as i8,
					&contract_code,
					nonce,
					&transaction_hash,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			println!("result.unwrap(): {:?}", result);
			//let contract_runtime_code: ContractCode =
			// hex::decode(get_contract_code(EVM_RUNTIME_ERC20_CONTRACT_CODE_PATH).unwrap()).
			// unwrap();
			let contract_instance_address = result.unwrap();

			let contract_instance_address: [u8; 20] = {
				let mut array = [0u8; 20];
				array.copy_from_slice(&contract_instance_address);
				array
			};
			let contract_instance = contract_instance_state
				.get_contract_instance(&contract_instance_address)
				.await
				.unwrap();
			let contract =
				contract_state.get_contract(&contract_instance.contract_address).await.unwrap();

			let arguments: ContractArgument = hex::decode("").unwrap();
			let caller = account_address.clone();
			let code_hash = H256::from_slice(Keccak256::digest(&contract_code).as_slice());
			let salt = generate_salt();
			let scheme = CreateScheme::Create2 { caller: caller.into(), code_hash, salt };
			let context = Context {
				/// Execution address.
				address: H160(account_address.clone()),
				/// Caller of the EVM.
				caller: H160(account_address.clone()),
				/// Apparent value of the EVM.
				apparent_value: U256::zero(),
			};
			let result = execute_contract_evm::ExecuteContract {}
				.execute_contract_function_call(
					&account_address,
					context,
					Gas::MAX,
					&cluster_address,
					&contract,
					&contract_instance,
					&vec![],
					&arguments,
					nonce,
					&transaction_hash,
					false,
					1,
					&[0u8; 32],
					0,
					Arc::new(&db_pool_conn),
					event_tx.clone(),
				)
				.await;

			//println!("execute_contract_function_call: {:?}", hex::encode(result.unwrap()));
			assert_eq!("", hex::encode(result.unwrap().result));
		}
	}
}
