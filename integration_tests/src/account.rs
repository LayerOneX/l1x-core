use super::server::Server;
use l1x_rpc::primitives::Address;
use l1x_rpc::rpc_model::AccountState;

use cli::types::Transaction;

#[derive(Clone)]
pub struct Account {
	pub address: Address,
	pub priv_key: String,
	pub server: Server,
}
impl Account {
	pub fn new(priv_key: String, server: Server) -> Self {
		let address_str = &l1x_rpc::get_address_from_privkey_str(&priv_key).unwrap();
		if address_str.len() != 40 {
			panic!("Invalid address: {}", address_str);
		}
		let address: [u8; 20] = hex::decode(address_str).unwrap().try_into().unwrap();

		Account { address, priv_key, server }
	}
	pub async fn transfer_tokens(&mut self, to: Address, amount: u128) -> String {
		let tx = Transaction::NativeTokenTransfer(cli::types::U8s::Bytes(to.to_vec()), amount);

		let address = hex::encode(self.address);
		let answer = self.server.send_tx(tx, address, self.priv_key.clone()).await;

		answer.hash
	}
	pub async fn get_account_state(&mut self) -> AccountState {
		let address = hex::encode(self.address);
		self.server.get_account_state(address).await
	}
}
