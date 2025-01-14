use anyhow::{anyhow, Error};
use l1x_rpc;
use primitives::{Address, Balance, BlockNumber};
use serde::{Deserialize, Serialize};

/// Serde wrappers for Transaction types, used to construct transaction payloads in the CLI

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[repr(i8)]
pub enum ContractType {
	L1XVM = 0,
	EVM = 1,
	XTALK = 2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[repr(i8)]
pub enum AccessType {
	PRIVATE = 0,
	PUBLIC = 1,
	RESTICTED = 2, /* Will be used in future to restrict the contract to be initiated by only
	                * specified addresses. */
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum U8s {
	Hex(String),
	Bytes(Vec<u8>),
	File(String),
	Text(String),
}

impl TryFrom<U8s> for Vec<u8> {
	type Error = Error;

	fn try_from(value: U8s) -> Result<Self, Self::Error> {
		Ok(match value {
			U8s::Hex(s) if s.is_empty() => vec![],
			U8s::Hex(s) => hex::decode(s)?,
			U8s::Bytes(v) => v,
			U8s::File(f) => std::fs::read(f)?,
			U8s::Text(t) => t.into_bytes(),
		})
	}
}

impl TryFrom<U8s> for Address {
	type Error = Error;

	fn try_from(value: U8s) -> Result<Self, Self::Error> {
		let bytes: Vec<u8> = value.try_into()?;
		if bytes.len() != 20 {
			Err(anyhow!("Invalid address {bytes:?}, length <> 20 bytes"))
		} else {
			let mut array = [0u8; 20];
			array.copy_from_slice(&bytes);
			Ok(array)
		}
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transaction {
	NativeTokenTransfer(U8s, Balance),
	SmartContractDeployment(AccessType, ContractType, U8s, Balance, U8s),
	SmartContractInit(U8s, U8s, Balance),
	SmartContractFunctionCall {
		contract_instance_address: U8s,
		function: U8s,
		arguments: U8s,
		deposit: Balance,
	},
	CreateStakingPool {
		contract_instance_address: Option<Address>,
		min_stake: Option<Balance>,
		max_stake: Option<Balance>,
		min_pool_balance: Option<Balance>,
		max_pool_balance: Option<Balance>,
		staking_period: Option<BlockNumber>,
	},
	Stake {
		pool_address: U8s,
		amount: Balance,
	},
	UnStake {
		pool_address: U8s,
		amount: Balance,
	},
}

impl TryFrom<Transaction> for l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType {
	type Error = Error;

	fn try_from(value: Transaction) -> Result<Self, Self::Error> {
		Ok(match value {
            Transaction::NativeTokenTransfer(address, balance) => {
                l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType::NativeTokenTransfer(
                    l1x_rpc::rpc_model::NativeTokenTransfer {
                        address: TryInto::<Vec<u8>>::try_into(address)?,
                        amount: balance.to_string(),
                    },
                )
            }
            Transaction::SmartContractDeployment(access_type, contract_type, code, deposit, salt) => {
                let access_type = match access_type {
                    AccessType::PRIVATE => l1x_rpc::rpc_model::AccessType::Private,
                    AccessType::PUBLIC => l1x_rpc::rpc_model::AccessType::Public,
                    AccessType::RESTICTED => l1x_rpc::rpc_model::AccessType::Resticted,
                };
                let contract_type = match contract_type {
                    ContractType::L1XVM => l1x_rpc::rpc_model::ContractType::L1xvm,
                    ContractType::XTALK => l1x_rpc::rpc_model::ContractType::Xtalk,
                    ContractType::EVM => l1x_rpc::rpc_model::ContractType::Evm,
                };
                l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType::SmartContractDeployment(
                    l1x_rpc::rpc_model::SmartContractDeploymentV2 {
                        access_type: access_type.into(),
                        contract_type: contract_type.into(),
                        contract_code: code.try_into()?,
                        deposit: deposit.to_string(),
                        salt: salt.try_into()?,
                    },
                )
            }
            Transaction::SmartContractInit(address, arguments, deposit) => {
                l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType::SmartContractInit(
                    l1x_rpc::rpc_model::SmartContractInitV2 {
                        contract_code_address: TryInto::<Vec<u8>>::try_into(address)?,
                        arguments: arguments.try_into()?,
                        deposit: deposit.to_string(),
                    },
                )
            }
            Transaction::SmartContractFunctionCall {
                contract_instance_address,
                function,
                arguments,
                deposit,
            } => l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType::SmartContractFunctionCall(
                l1x_rpc::rpc_model::SmartContractFunctionCallV2 {
                    contract_instance_address: TryInto::<Vec<u8>>::try_into(contract_instance_address)?,
                    function_name: TryInto::<Vec<u8>>::try_into(function)?,
                    arguments: arguments.try_into()?,
                    deposit: deposit.to_string(),
                },
            ),
            Transaction::CreateStakingPool {
                contract_instance_address,
                min_stake,
                max_stake,
                min_pool_balance,
                max_pool_balance,
                staking_period,
            } => {
                let contract_instance_address: Option<Vec<u8>> = contract_instance_address
                    .map(TryInto::try_into)
                    .transpose()?;
                l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType::CreateStakingPool(
                    l1x_rpc::rpc_model::CreateStakingPool {
                        contract_instance_address,
                        min_stake:  min_stake.map(|x| x.to_string()),
                        max_stake: max_stake.map(|x| x.to_string()),
                        min_pool_balance: min_pool_balance.map(|x| x.to_string()),
                        max_pool_balance: max_pool_balance.map(|x| x.to_string()),
                        staking_period: staking_period.map(|x| x.to_string()),
                    },
                )
            }
            Transaction::Stake {
                pool_address,
                amount,
            } => l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType::Stake(l1x_rpc::rpc_model::Stake {
                pool_address: TryInto::<Vec<u8>>::try_into(pool_address)?,
                amount: amount.to_string()
            }),
            Transaction::UnStake {
                pool_address,
                amount,
            } => l1x_rpc::rpc_model::submit_transaction_request_v2::TransactionType::Unstake(l1x_rpc::rpc_model::UnStake {
                pool_address: TryInto::<Vec<u8>>::try_into(pool_address)?,
                amount: amount.to_string(),
            }),
        })
	}
}

/// Root of the transaction deployment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionDeployment {
	pub private_key: U8s,
	pub public_key: U8s,
	pub transaction: Transaction,
}

// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// pub struct SmartContractReadOnlyFunctionCall {
//     pub contract_instance_address: U8s,
//     pub function: U8s,
//     pub arguments: U8s,
// }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartContractReadOnlyFunctionCall {
	pub contract_instance_address: U8s,
	pub function: U8s,
	pub arguments: U8s,
}

impl TryFrom<SmartContractReadOnlyFunctionCall>
	for l1x_rpc::rpc_model::SmartContractReadOnlyCallRequest
{
	type Error = Error;

	fn try_from(value: SmartContractReadOnlyFunctionCall) -> Result<Self, Self::Error> {
		Ok(l1x_rpc::rpc_model::SmartContractReadOnlyCallRequest {
			call: Some(l1x_rpc::rpc_model::SmartContractFunctionCall {
				contract_address: TryInto::<Vec<u8>>::try_into(value.contract_instance_address)?,
				function_name: TryInto::<Vec<u8>>::try_into(value.function)?,
				arguments: value.arguments.try_into()?,
			}),
		})
	}
}
