#[cfg(test)]
mod account;

#[cfg(test)]
mod contract;

#[cfg(test)]
mod server;

#[cfg(test)]
mod tests {
	use super::*;
	use serde::Deserialize;
	use std::path::PathBuf;
	use std::sync::Once;
	use toml::from_str;

	static INIT: Once = Once::new();
	static mut CONFIG: Option<Config> = None;

	#[derive(Debug, Deserialize, Clone)]
	struct Config {
		private_key_1: String,
		private_key_2: String,
		grpc_endpoint: String,
	}

	fn load_config() -> &'static Config {
		unsafe {
			INIT.call_once(|| {
				let config_str =
					std::fs::read_to_string("config.toml").expect("Failed to read config file");
				CONFIG = from_str(&config_str).expect("Failed to parse config file");
			});
			CONFIG.as_ref().unwrap()
		}
	}

	#[tokio::test]
	async fn test_contract() {
		let config = load_config().clone();
		let server = server::Server::connect(config.grpc_endpoint).await;

		let account = account::Account::new(config.private_key_1, server.clone());

		let path_to_object_file =
			PathBuf::from("./fixtures/l1x-test-contract/target/l1x/release/l1x_test_contract.o");
		let mut contract_code = contract::ContractCode::new(path_to_object_file, account.clone());

		contract_code.deploy().await.unwrap();
		println!("Contract deployed at {:?}", hex::encode(contract_code.address.unwrap()));
		tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

		let mut contract_instance = contract_code.initialize(None).await;
		println!("Contract initialized at {:?}", hex::encode(contract_instance.address));
		tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

		let hash = contract_instance
			.call("set_counter".to_string(), vec!["value".to_string(), "42".to_string()])
			.await;
		println!("Transaction succeeded. Hash: {}", hash);

		let response = contract_instance.read_only_call("get_counter".to_string(), vec![]).await;
		println!("Read only call suceeded. Response: {:?}", response);
	}

	#[tokio::test]
	async fn test_transfer() {
		let config = load_config().clone();
		let server = server::Server::connect(config.grpc_endpoint).await;

		let mut account = account::Account::new(config.private_key_1, server.clone());
		let account2 = account::Account::new(config.private_key_2, server.clone());

		let response = account.transfer_tokens(account2.address, 100).await;
		println!("Token transfer suceeded. Response: {:?}", response);
	}

	#[tokio::test]
	async fn get_account_state() {
		let config = load_config().clone();
		let server = server::Server::connect(config.grpc_endpoint).await;

		let mut account = account::Account::new(config.private_key_1, server.clone());

		let _account_state = account.get_account_state().await;
	}
}
