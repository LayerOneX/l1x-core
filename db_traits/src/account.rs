use anyhow::Error;
use async_trait::async_trait;
use primitives::*;
use system::account::Account;

#[async_trait]
pub trait AccountState {
	async fn update_balance(&self, account: &Account) -> Result<(), Error>;
	async fn increment_nonce(&self, address: &Address) -> Result<(), Error>;

	async fn get_account(&self, address: &Address) -> Result<Account, Error>;

	async fn get_nonce(&self, address: &Address) -> Result<Nonce, Error>;

	async fn get_balance(&self, address: &Address) -> Result<Balance, Error>;

	async fn is_valid_account(&self, address: &Address) -> Result<bool, Error>;
}
