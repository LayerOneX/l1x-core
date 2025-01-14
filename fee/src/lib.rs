pub mod gas_station;
pub mod fee_manager;

use primitives::{Address, Balance};
use system::{
	contract::ContractType,
	transaction::{Transaction, TransactionType},
};

/// Fees for a contract
#[derive(Debug, Clone)]
pub struct ContractFee {
	pub deployment: Balance,
	pub deployment_per_byte: Balance,
	pub init: Balance,
	pub init_per_byte: Balance,
	pub method_call: Balance,
	pub method_call_per_byte: Balance,
}

/// All possible Fees
#[derive(Debug, Clone)]
pub struct FeeConfig {
	// Transaction fees are always charged
	pub transaction: Balance,          // constant fee for each trasaction
	pub transaction_per_byte: Balance, // dinamic fee that depends on the size of the transaction

	// Transaction type fees
	pub token_transfer: Balance,
	pub create_staking_pool: Balance,
	pub stake: Balance,
	pub unstake: Balance,
	pub staking_pool_contract: Balance,

	// The following Fees are dinamic and depend on a contract type.
	// These fees are charged before contract execution and not related to Gas allocated for the
	// contract execution
	pub l1xvm_contract: ContractFee,
	pub l1xevm_contract: ContractFee,
	pub xtalk_contract: ContractFee,

	// All charged fees will be trasfered to this address
	pub fee_recipient: Address,
}

impl FeeConfig {
	/// Calculates Transaction Fee depending on the specified transaction size
	///
	/// Returns None If integer overflow happend and the calcukate fee amount otherwise
	pub fn transaction_size_fee(&self, tx: &Transaction) -> Option<Balance> {
		let size = match &tx.transaction_type {
			TransactionType::NativeTokenTransfer(..) => 1,
			TransactionType::CreateStakingPool { .. } => 1,
			TransactionType::Stake { .. } => 1,
			TransactionType::UnStake { .. } => 1,
			TransactionType::StakingPoolContract { .. } => 1,
			TransactionType::SmartContractDeployment {
				access_type: _,
				contract_type: _,
				contract_code,
				deposit: _,
				salt,
			} => contract_code.len().checked_add(salt.len())?,
			TransactionType::SmartContractInit {
				contract_code_address: _,
				arguments,
				deposit: _,
			} => arguments.len(),
			TransactionType::SmartContractFunctionCall {
				contract_instance_address: _,
				function,
				arguments,
				deposit: _,
			} => function.len().checked_add(arguments.len()).unwrap_or(usize::MAX),
		};

		let per_byte = self.transaction_per_byte.checked_mul(size as _)?;
		self.transaction.checked_add(per_byte)
	}

	/// Calculates Contract Fee. Fee may depend on input parameters size, contract size.
	///
	/// Returns None If integer overflow happend and the calcukate fee amount otherwise
	pub fn contract_fee(&self, tx: &Transaction, contract_type: &ContractType) -> Option<Balance> {
		match &tx.transaction_type {
			TransactionType::SmartContractDeployment {
				access_type: _,
				contract_type: _,
				contract_code,
				deposit: _,
				salt: _,
			} => match contract_type {
				ContractType::EVM => self
					.l1xevm_contract
					.deployment_per_byte
					.checked_mul(contract_code.len() as _)?
					.checked_add(self.l1xevm_contract.deployment),
				ContractType::L1XVM => self
					.l1xvm_contract
					.deployment_per_byte
					.checked_mul(contract_code.len() as _)?
					.checked_add(self.l1xvm_contract.deployment),
				ContractType::XTALK => self
					.xtalk_contract
					.deployment_per_byte
					.checked_mul(contract_code.len() as _)?
					.checked_add(self.xtalk_contract.deployment),
			},
			TransactionType::SmartContractInit {
				contract_code_address: _,
				arguments,
				deposit: _,
			} => match contract_type {
					ContractType::EVM => self
						.l1xevm_contract
						.init_per_byte
						.checked_mul(arguments.len() as _)?
						.checked_add(self.l1xevm_contract.init),
					ContractType::L1XVM => self
						.l1xvm_contract
						.init_per_byte
						.checked_mul(arguments.len() as _)?
						.checked_add(self.l1xvm_contract.init),
					ContractType::XTALK => self
						.xtalk_contract
						.init_per_byte
						.checked_mul(arguments.len() as _)?
						.checked_add(self.xtalk_contract.init),
				},
			TransactionType::SmartContractFunctionCall {
				contract_instance_address: _,
				function,
				arguments,
				deposit: _
			} => {
				let size = function.len().checked_add(arguments.len())? as Balance;
				match contract_type {
					ContractType::EVM => self
						.l1xevm_contract
						.method_call_per_byte
						.checked_mul(size)?
						.checked_add(self.l1xevm_contract.method_call),
					ContractType::L1XVM => self
						.l1xvm_contract
						.method_call_per_byte
						.checked_mul(size)?
						.checked_add(self.l1xvm_contract.method_call),
					ContractType::XTALK => self
						.xtalk_contract
						.method_call_per_byte
						.checked_mul(size)?
						.checked_add(self.xtalk_contract.method_call),
				}
			},
			_ => None,
		}
	}

	/// Returns a default config
	pub fn config(fee_recipient: Address) -> Self {
		FeeConfig {
			transaction: 1,
			transaction_per_byte: 1,

			token_transfer: 1,
			create_staking_pool: 1,
			stake: 1,
			unstake: 1,
			staking_pool_contract: 1,

			l1xvm_contract: ContractFee {
				deployment: 1,
				deployment_per_byte: 0,
				init: 1,
				init_per_byte: 0,
				method_call: 1,
				method_call_per_byte: 0,
			},

			l1xevm_contract: ContractFee {
				deployment: 1,
				deployment_per_byte: 0,
				init: 1,
				init_per_byte: 0,
				method_call: 1,
				method_call_per_byte: 0,
			},

			xtalk_contract: ContractFee {
				deployment: 1,
				deployment_per_byte: 0,
				init: 1,
				init_per_byte: 0,
				method_call: 1,
				method_call_per_byte: 0,
			},

			fee_recipient,
		}
	}
}
