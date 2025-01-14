use anyhow::Error;
use primitives::*;

pub struct CallContractOutcome {
	pub result: EventData,
	pub burnt_gas: Gas,
}

pub enum L1XNativeFunction {
	Balance(Address),
	Transfer {
		from_address: Address,
		to_address: Address,
		amount: Balance,
	},
	CreateStakingPool {
		owner_address: Address,
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
	},
	Stake {
		from_address: Address,
		pool_address: Address,
		amount: Balance,
	},
	UnStake {
		to_address: Address,
		pool_address: Address,
		amount: Balance,
	},
	StakeBalance {
		account_address: Address,
		pool_address: Address,
	},
}

pub enum L1XNativeFunctionResult {
	Balance(Balance),
	Address(Address),
	EventData(EventData),
	Bool(bool),
	NoOp,
}

pub trait VMContractTrait {
	fn contract_instance_owner_address_of(
		&self,
		contract_instance_address: Address,
	) -> Result<Address, Error>;

	fn contract_code_owner_address_of(
		&self,
		contract_code_address: Address,
	) -> Result<Address, Error>;

	fn contract_code_address_of(
		&self,
		contract_instance_address: Address,
	) -> Result<Address, Error>;

	fn storage_read(
		&mut self,
		key: &ContractInstanceKey,
	) -> Result<Option<ContractInstanceValue>, Error>;

	fn storage_remove(&mut self, key: &ContractInstanceKey) -> Result<(), Error>;

	fn storage_write(
		&mut self,
		key: ContractInstanceKey,
		value: ContractInstanceValue,
	) -> Result<(), Error>;

	fn get_balance(&self, address: Address) -> Result<Balance, Error>;

	fn transfer_token(
		&mut self,
		from_address: Address,
		to_address: Address,
		amount: Balance,
	) -> Result<(), Error>;

	fn call_contract(
		&mut self,
		contract_instance_address: Address,
		function: ContractFunction,
		arguments: ContractArgument,
		gas_limit: Gas,
		deposit: Balance,
		readonly: bool,
	) -> Result<CallContractOutcome, Error>;

	fn emit_event(&mut self, event_data: EventData) -> Result<(), Error>;

	// Inter cluster is not yet implemented
	fn execute_remote_contract(
		&mut self,
		_cluster_address: Address,
		_contract_instance_address: Address,
		_function: ContractFunction,
		_arguments: Vec<ContractArgument>,
	) -> Result<TransactionHash, Error>;
}
