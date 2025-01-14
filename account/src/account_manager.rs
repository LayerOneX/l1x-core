use crate::account_state::AccountState;
use anyhow::{anyhow, Error};
use db::utils::big_int::{FromBigInt, ToBigInt};
use primitives::{Address, Balance, Nonce};
use system::account::Account;

/// Manages operations related to user accounts.
pub struct AccountManager {
	/// The account associated with the manager.
	pub account: Account,
}

impl<'a> AccountManager {
	/// Creates a new `AccountManager` with the specified address and initializes it in the account
	/// state.
	///
	/// # Arguments
	///
	/// * `address` - The address of the account to be managed.
	/// * `account_state` - The account state implementation to use for initialization.
	///
	/// # Returns
	///
	/// * `Result<AccountManager, Error>` - A `Result` containing either the initialized
	///   `AccountManager` or an error.
	pub async fn new(
		address: &Address,
		account_state: &AccountState<'a>,
	) -> Result<AccountManager, Error> {
		let account = Account::new(*address);
		account_state.create_account(&account).await?;
		let account_manager = AccountManager { account };
		Ok(account_manager)
	}

	/// Creates a new system account and initializes it in the account state.
	///
	/// # Arguments
	///
	/// * `address` - The address of the system account to be managed.
	/// * `account_state` - The account state implementation to use for initialization.
	///
	/// # Returns
	///
	/// * `Result<AccountManager, Error>` - A `Result` containing either the initialized
	///   `AccountManager` or an error.
	pub async fn new_system(
		address: &Address,
		account_state: &AccountState<'a>,
	) -> Result<AccountManager, Error> {
		let account = Account::new_system(*address);
		account_state.create_account(&account).await?;
		let account_manager = AccountManager { account };
		Ok(account_manager)
	}

	/// Gets the balance of the managed account.
	pub fn get_balance(&self) -> Balance {
		self.account.balance
	}

	/// Gets the current nonce of the managed account.
	///
	/// # Arguments
	///
	/// * `_account_state` - The account state implementation. Unused in this method.
	///
	/// # Returns
	///
	/// * `Result<Nonce, Error>` - The current nonce of the managed account, wrapped in a `Result`.
	pub async fn get_current_nonce(
		&mut self,
		_account_state: &AccountState<'a>,
	) -> Result<Nonce, Error> {
		Ok(self.account.nonce)
	}

	/// Transfers funds from the managed account to another account.
	///
	/// # Arguments
	///
	/// * `to` - The address of the recipient account.
	/// * `amount` - The amount of funds to transfer.
	/// * `account_state` - The account state implementation to use for the transfer operation.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A `Result` indicating success or failure of the transfer operation.
	pub async fn transfer(
		&mut self,
		to: &Address,
		amount: &Balance,
		account_state: &AccountState<'a>,
	) -> Result<(), Error> {
		if !self.has_sufficient_balance(amount) {
			return Err(anyhow!("Insufficient balance, address {}", hex::encode(&self.account.address)));
		}

		// Deduct the transferred amount from the sender's account balance
		let account_balance = self
			.account
			.balance
			.get_big_int()
			.checked_sub(&amount.get_big_int())
			.ok_or(anyhow!("Error Subtracting balance"))?;
		self.account.balance = u128::from_big_int(&account_balance);

		// Update the sender's account balance in the account state
		account_state.update_balance(&self.account).await?;

		// Retrieve the recipient's account or create a new one if it doesn't exist
		let mut to_account = if !account_state.is_valid_account(to).await? {
			Account::new(*to)
		} else {
			account_state.get_account(to).await?
		};

		// Add the transferred amount to the recipient's account balance
		let to_account_balance = to_account
			.balance
			.get_big_int()
			.checked_add(&amount.get_big_int())
			.ok_or(anyhow!("Error Adding balance"))?;
		to_account.balance = u128::from_big_int(&to_account_balance);

		// Update the recipient's account balance in the account state
		account_state.update_balance(&to_account).await?;

		Ok(())
	}

	/// Mints (increases) the balance of the current account by the specified amount.
	///
	/// # Arguments
	///
	/// * `amount`: The amount to mint (increase) the balance by.
	/// * `account_state`: A reference to the account state instance used to update the balance.
	///
	/// # Returns
	///
	/// Returns `Ok(())` if the minting operation is successful, or an `Error` otherwise.
	pub async fn mint(
		&mut self,
		amount: Balance,
		account_state: &AccountState<'a>,
	) -> Result<(), Error> {
		// Calculate the new balance by adding the specified amount to the current balance
		let account_balance = self
			.account
			.balance
			.get_big_int()
			.checked_add(&amount.get_big_int())
			.ok_or(anyhow!("Error Adding balance"))?;

		// Update the balance of the current account
		self.account.balance = u128::from_big_int(&account_balance);

		// Update the balance in the account state
		account_state.update_balance(&self.account).await?;

		Ok(())
	}

	/// Checks if the managed account has a sufficient balance for a transfer operation.
	///
	/// # Arguments
	///
	/// * `amount` - The amount of funds to transfer.
	///
	/// # Returns
	///
	/// * `bool` - `true` if the account has sufficient balance, `false` otherwise.
	pub fn has_sufficient_balance(&self, amount: &Balance) -> bool {
		self.account.balance >= *amount
	}

	/// Increments the nonce of the account associated with the specified address.
	///
	/// # Arguments
	///
	/// * `address` - The address of the account whose nonce is to be incremented.
	/// * `account_state` - The account state implementation to use for the nonce increment
	///   operation.
	///
	/// # Returns
	///
	/// * `Result<(), Error>` - A `Result` indicating success or failure of the nonce increment
	///   operation.
	pub async fn increment_nonce(
		address: &Address,
		account_state: &AccountState<'a>,
	) -> Result<(), Error> {
		account_state.increment_nonce(address).await
	}
}
