use anyhow::{anyhow, Error};
use async_scoped::TokioScope;
use compile_time_config::{config::FEE_RECIPIENT_MASTER_ADDRESS, EVM_GAS_PRICE, GAS_PRICE};
use fee::{gas_station::GasStation, FeeConfig};
use log::info;
use primitives::{Address, Balance, Gas};
use state::UpdatedState;
use system::{contract::ContractType, transaction::Transaction};

use crate::execute_token::ExecuteToken;

pub struct SystemFeeConfig {
	pub evm_gas_price: Balance,
	pub gas_price: Balance,
	pub fee_recipient_address: Address,
}

impl SystemFeeConfig {
	pub fn new() -> Self {
		Self {
			evm_gas_price: EVM_GAS_PRICE,
			gas_price: GAS_PRICE,
			fee_recipient_address: FEE_RECIPIENT_MASTER_ADDRESS,
		}
	}

	pub fn gas_station_from_system_config() -> GasStation {
		let system_config = Self::new();

		GasStation::from_fixed_price(
			system_config.gas_price,
			system_config.evm_gas_price,
		)
	}
}

pub struct ExecuteFee {
	transaction: Transaction,     // Transaction this instance was created for
	locked_fee_limit: Balance,    // locked fee amount for the prepaid gas
	available_fee_limit: Balance, // available fee amount can be charged anytime
	fee_config: FeeConfig,
	gas_station: GasStation,
}

impl ExecuteFee {
	pub fn new(transaction: &Transaction, system_fee_config: SystemFeeConfig) -> Self {
		Self {
			transaction: transaction.clone(),
			locked_fee_limit: 0,
			available_fee_limit: transaction.fee_limit,
			fee_config: FeeConfig::config(system_fee_config.fee_recipient_address),
			gas_station: GasStation::from_fixed_price(
				system_fee_config.gas_price as _,
				system_fee_config.evm_gas_price as _,
			),
		}
	}

	/// Returns `FeeConfig` used in this instance
	pub fn fee_config(&self) -> FeeConfig {
		self.fee_config.clone()
	}

	/// Returns how much fee can be charged within the fee limit
	pub fn available_limit(&self) -> Balance {
		self.available_fee_limit
	}

	/// Returns how much fee is locked
	pub fn locked_limit(&self) -> Balance {
		self.locked_fee_limit
	}

	/// Returns a reference to the gas station
	pub fn gas_station(&self) -> &GasStation {
		&self.gas_station
	}

	/// Safely substract available fee limit.
	///
	/// Returns `Err` the limit is reached
	fn sub_available_limit(&mut self, amount: Balance) -> anyhow::Result<()> {
		self.available_fee_limit = self
			.available_fee_limit
			.checked_sub(amount)
			.ok_or(anyhow!("Reached Fee Limit"))?;
		Ok(())
	}

	/// Safely substract locked fee limit.
	///
	/// Returns `Err` if locked fee limit overflow happened
	fn sub_locked_limit(&mut self, amount: Balance) -> anyhow::Result<()> {
		self.locked_fee_limit = self
			.locked_fee_limit
			.checked_sub(amount)
			.ok_or(anyhow!("Locked Fee limit overflow"))?;
		Ok(())
	}

	/// Prepays Gas using all available fee limit. The avalilable fee limit becomes locked
	///
	/// Returns Gas amount or Err if could not lock fee limit
	pub fn prepay_gas_and_lock<'a>(
		&mut self,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<Gas> {
		self.lock_within_limit(self.available_fee_limit, account_address, updated_state)?;

		let prepaid_gas = self.gas_station.buy_gas(self.locked_fee_limit);
		info!(
			"Fee: Prepay Gas within the available limit: {} Gas for {} tokens, gas price {}",
			prepaid_gas,
			self.locked_fee_limit,
			self.gas_station.gas_price()
		);

		Ok(prepaid_gas)
	}

	/// Sells the burnt Gas. Locked Fee is unlocked. The fee for the burnt Gas is charged
	pub fn sell_gas_and_unlock<'a>(
		&mut self,
		burnt_gas: Gas,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<Balance> {
		self.unlock_limit(self.locked_fee_limit, account_address, updated_state)?;

		let fee_amount = self.gas_station.sell_gas(burnt_gas);
		info!(
			"Fee: Refund unused Gas: burnt gas {}, available limit {}, charge {} tokens for the burnt gas, gas price {}",
			burnt_gas, self.available_fee_limit, fee_amount, self.gas_station.gas_price()
		);
		// Charge and return the charged fee amount
		Ok(self.charge_fee(account_address, fee_amount, updated_state)?)
	}

	pub fn convert_from_gas_to_fee(&self, gas: Gas) -> Balance {
		self.gas_station.sell_gas(gas)
	}

	pub fn charge_transaction_size_fee<'a>(
		&mut self,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<Balance, Error> {
		if let Some(fee_amount) = self.fee_config.transaction_size_fee(&self.transaction) {
			info!(
			"Fee: Charge Transaction Fee: {}, available limit {}",
			fee_amount, self.available_fee_limit
		);
			// Charge and return the charged fee amount
			Ok(self.charge_fee(account_address, fee_amount, updated_state)?)
		} else {
			Err(anyhow!("Can't charge Transaction Fee: integer overflow"))
		}
	}

	pub fn get_transaction_size_fee<'a>(
		&mut self,
	) -> Result<Balance,Error> {
		match self.fee_config.transaction_size_fee(&self.transaction) {
			Some(fee) => {
				Ok(fee)
			} None => {
				Err(anyhow!("Can't get transaction size fee"))
			}
		}
	}

	pub fn get_token_transfer_fee<'a>(
		&mut self,
	) -> Balance {
		self.fee_config.token_transfer
	}

	pub fn get_token_transfer_tx_total_fee<'a>(
		&mut self,
	) -> Result<Balance,Error> {
		Ok(self.fee_config.token_transfer + self.get_transaction_size_fee()?)
	}
	
	pub fn charge_token_transfer_fee<'a>(
		&mut self,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<Balance> {
		// Log the transfer fee and available fee limit
		info!(
			"Fee: Charge Transfer Fee: {}, available limit {}",
			self.fee_config.token_transfer, self.available_fee_limit
		);

		// Charge and return the charged fee amount
		Ok(self.charge_fee(account_address, self.fee_config.token_transfer, updated_state)?)
	}

	pub fn charge_create_staking_pool_fee<'a>(
		&mut self,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<Balance> {
		info!(
			"Fee: Charge Creating Staking Pool Fee: {}, avalable limit {}",
			self.fee_config.create_staking_pool, self.available_fee_limit
		);
		// Charge and return the charged fee amount
		Ok(self.charge_fee(account_address, self.fee_config.create_staking_pool, updated_state)?)
	}

	pub fn get_create_staking_pool_fee<'a>(
		&mut self,
	) -> Balance {
		self.fee_config.create_staking_pool
	}

	pub fn charge_stake_fee<'a>(
		&mut self,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<Balance> {
		info!(
			"Fee: Charge Stake Fee: {}, avalable limit {}",
			self.fee_config.stake, self.available_fee_limit
		);
		// Charge and return the charged fee amount
		Ok(self.charge_fee(account_address, self.fee_config.stake, updated_state)?)
	}

	pub fn get_stake_fee<'a>(
		&mut self,
	) -> Balance {
		self.fee_config.stake
	}

	pub fn charge_unstake_fee<'a>(
		&mut self,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<Balance> {
		info!(
			"Fee: Charge UnStake Fee: {}, avalable limit {}",
			self.fee_config.unstake, self.available_fee_limit
		);
		// Charge and return the charged fee amount
		Ok(self.charge_fee(account_address, self.fee_config.unstake, updated_state)?)
	}

	pub fn get_unstake_fee<'a>(
		&mut self,
	) -> Balance {
		self.fee_config.unstake
	}

	pub fn charge_staking_pool_contract_fee<'a>(
		&mut self,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<Balance> {
		info!(
			"Fee: Charge Staking Pool Contract Fee: {}, avalable limit {}",
			self.fee_config.staking_pool_contract, self.available_fee_limit
		);
		// Charge and return the charged fee amount
		Ok(self.charge_fee(account_address, self.fee_config.staking_pool_contract, updated_state)?)
	}

	pub fn get_staking_pool_contract_fee<'a>(
		&mut self,
	) -> Balance {
		self.fee_config.staking_pool_contract
	}

	pub fn charge_contract_fee<'a>(
		&mut self,
		contract_type: &ContractType,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> Result<Balance, anyhow::Error> {
		if let Some(fee_amount) = self.fee_config.contract_fee(&self.transaction, contract_type) {
			info!(
				"Fee: Charge Contract Fee: {}, avalable limit {}",
				fee_amount, self.available_fee_limit
			);
			// Charge and return the charged fee amount
			Ok(self.charge_fee(account_address, fee_amount, updated_state)?)
		} else {
			Err(anyhow!("Can't charge Transaction Fee: interger overflow"))
		}
	}

	pub fn get_contract_fee<'a>(
		&mut self,
		contract_type: &ContractType,
	) -> Result<Balance, Error> {
		match self.fee_config.contract_fee(&self.transaction, contract_type) {
			Some(fee) => {
				Ok(fee)
			} None => {
				Err(anyhow!("Can't get contract fee"))
			}
		}
	}

	fn charge_fee<'a>(
		&mut self,
		account_address: &Address,
		fee_amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<Balance, Error>  {
		if fee_amount == 0 {
			return Ok(fee_amount)
		}

		self.sub_available_limit(fee_amount)?;

		let result = TokioScope::scope_and_block(|scope| {
			scope.spawn_blocking(move || {
				// Create a new Tokio runtime for the async block
				let rt = tokio::runtime::Runtime::new().expect("Failed to create a runtime");
				// Use the runtime to block on the async operation
				rt.block_on(async {
					ExecuteToken::just_transfer_tokens(
						account_address,
						&self.fee_config.fee_recipient,
						fee_amount,
						updated_state,
					)
					.map_err(|e| anyhow!("Can't charge Transaction Fee: {:?}", e))
				})
			})
		});

		match result.1.into_iter().next() {
			Some(result) => {
				match result {
					Ok(_) => Ok(fee_amount),
					Err(e) => Err(anyhow!("Charge fee error. Failed during token transfer execution: {:?}", e)),
				}
			}
			None => {
				Err(anyhow!("Charge fee error. TokioScope execution error"))
			}
		}
	}

	fn lock_within_limit<'a>(
		&mut self,
		amount: Balance,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<()> {
		if amount == 0 {
			return Ok(())
		}

		self.sub_available_limit(amount)?;
		self.locked_fee_limit += amount;

		let result = TokioScope::scope_and_block(|scope| {
			scope.spawn_blocking(move || {
				// Create a new Tokio runtime for the async block
				let rt = tokio::runtime::Runtime::new().expect("Failed to create a runtime");
				// Use the runtime to block on the async operation
				rt.block_on(async {
					ExecuteToken::just_transfer_tokens(
						account_address,
						&self.fee_config.fee_recipient,
						amount,
						updated_state
					)
					.map_err(|e| anyhow!("Can't lock Fee Limit: {:?}", e))
				})
			})
		});

		result.1.into_iter().next().unwrap().unwrap()
	}

	fn unlock_limit<'a>(
		&mut self,
		amount: Balance,
		account_address: &Address,
		updated_state: &mut UpdatedState,
	) -> anyhow::Result<()> {
		if amount == 0 {
			return Ok(())
		}

		self.sub_locked_limit(amount)?;
		self.available_fee_limit += amount;

		let result = TokioScope::scope_and_block(|scope| {
			scope.spawn_blocking(move || {
				// Create a new Tokio runtime for the async block
				let rt = tokio::runtime::Runtime::new().expect("Failed to create a runtime");
				// Use the runtime to block on the async operation
				rt.block_on(async {
					ExecuteToken::just_transfer_tokens(
						&self.fee_config.fee_recipient,
						account_address,
						amount,
						updated_state
					)
					.map_err(|e| anyhow!("Can't unlock Fee Limit: {:?}", e))
				})
			})
		});

		result.1.into_iter().next().unwrap().unwrap()
	}
}
