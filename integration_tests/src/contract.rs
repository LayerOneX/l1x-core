use super::account::Account;
use anyhow::anyhow;
use cli::types::{SmartContractReadOnlyFunctionCall, Transaction};
use l1x_rpc::primitives::Address;
use std::path::PathBuf;

pub struct ContractCode {
	pub path: PathBuf,
	pub account: Account,
	pub address: Option<Address>,
}
impl ContractCode {
	pub fn new(object_file: PathBuf, account: Account) -> Self {
		ContractCode { path: object_file, account, address: None }
	}

	pub async fn deploy(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		let path = self.path.to_str().ok_or(anyhow!("Invalid path"))?;

		let tx = Transaction::SmartContractDeployment(
			cli::types::AccessType::PRIVATE,
			cli::types::ContractType::L1XVM,
			cli::types::U8s::File(path.to_string()),
			0,
			cli::types::U8s::Text("00000000000000000000000000000000".to_string()),
		);

		let address = hex::encode(self.account.address);
		let answer = self.account.server.send_tx(tx, address, self.account.priv_key.clone()).await;

		assert!(answer.contract_address.is_some());

		let address = hex::decode(answer.contract_address.unwrap())?.try_into().unwrap();
		self.address = Some(address);
		Ok(())
	}

	pub async fn initialize(&mut self, text: Option<String>) -> ContractInstance {
		assert!(self.address.is_some());
		let text = text.unwrap_or("{}".to_string());

		let tx = Transaction::SmartContractInit(
			cli::types::U8s::Bytes(self.address.unwrap().to_vec()),
			cli::types::U8s::Text(text),
		);

		let address = hex::encode(self.account.address);
		let answer = self.account.server.send_tx(tx, address, self.account.priv_key.clone()).await;
		let address = hex::decode(answer.contract_address.unwrap()).unwrap().try_into().unwrap();

		ContractInstance { address, account: self.account.clone() }
	}
}

pub struct ContractInstance {
	pub address: Address,
	pub account: Account,
}
impl ContractInstance {
	pub async fn call(&mut self, func_name: String, arguments: Vec<String>) -> String {
		let json_string = format!(
			"{{{}}}",
			arguments
				.iter()
				.enumerate()
				.map(|(i, arg)| {
					let is_last = i == arguments.len() - 1;
					format!(r#"\\"{}\\": \\"{}\\"{}"#, arg, arg, if is_last { "" } else { "," })
				})
				.collect::<Vec<_>>()
				.join("")
		);
		let tx = Transaction::SmartContractFunctionCall {
			contract_instance_address: cli::types::U8s::Bytes(self.address.to_vec()),
			function: cli::types::U8s::Text(func_name),
			arguments: cli::types::U8s::Text(json_string),
		};

		let address = hex::encode(self.account.address);
		let answer = self.account.server.send_tx(tx, address, self.account.priv_key.clone()).await;

		answer.hash
	}
	pub async fn read_only_call(&mut self, func_name: String, arguments: Vec<String>) -> String {
		let json_string = format!(
			"{{{}}}",
			arguments
				.iter()
				.enumerate()
				.map(|(i, arg)| {
					let is_last = i == arguments.len() - 1;
					format!(r#"\\"{}\\": \\"{}\\"{}"#, arg, arg, if is_last { "" } else { "," })
				})
				.collect::<Vec<_>>()
				.join("")
		);

		let read_only_request = SmartContractReadOnlyFunctionCall {
			contract_instance_address: cli::types::U8s::Bytes(self.address.to_vec()),
			function: cli::types::U8s::Text(func_name),
			arguments: cli::types::U8s::Text(json_string),
		};
		let read_only_request: l1x_rpc::rpc_model::SmartContractReadOnlyCallRequest =
			read_only_request.try_into().unwrap();

		let answer = self.account.server.send_readonly_tx(read_only_request).await;

		serde_json::from_slice(&answer.result).unwrap()
	}
}
