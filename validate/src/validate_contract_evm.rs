use crate::validate_common::{ContractValidator, ValidateCommon};
use anyhow::Error;
use anyhow::anyhow;
use async_trait::async_trait;
use num_traits::Zero;
use contract_instance::contract_instance_state::ContractInstanceState;
use db::db::DbTxConn;
use primitives::*;
use system::transaction::{Transaction, TransactionType};

pub struct ValidateContract;

#[async_trait]
impl<'a> ContractValidator<'a> for ValidateContract {
	async fn validate_contract_deployment(
		&self,
		transaction: &Transaction,
		_contract_code: &ContractCode,
		sender: &Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		// FIXME: Temporarily bypassing contract code validation - needs implementation!
		ValidateCommon::validate_common(transaction, sender, db_pool_conn).await?;
		Ok(())
	}

	async fn validate_contract_init(
		&self,
		_transaction: &Transaction,
		_contract_address: &Address,
		_arguments: &ContractArgument,
		_sender: &Address,
		_db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}

	async fn validate_contract_function_call(
		&self,
		transaction: &Transaction,
		contract_instance_address: &Address,
		_function_name: &ContractFunction,
		arguments: &ContractArgument,
		sender: &Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		ValidateCommon::validate_common(transaction, sender, db_pool_conn).await?;
		let contract_instance_state = ContractInstanceState::new(db_pool_conn).await?;
		if let TransactionType::SmartContractFunctionCall { deposit, .. } = &transaction.transaction_type {
			let is_valid_contract_address = contract_instance_state.is_valid_contract_instance(contract_instance_address).await?;
			// if there is no contract and arguments then it's a native token transfer
			let is_native_token_transfer = !is_valid_contract_address && arguments.is_empty();
			// In case of native token transfer deposit should be greater than zero
			if is_native_token_transfer && deposit.is_zero() {
				return Err(anyhow!("Deposit should be greater than 0"));
			}
			// In case of function call contract address should be valid
			if !is_native_token_transfer && !is_valid_contract_address {
				return Err(anyhow!("Can't find the contract address: {}", hex::encode(contract_instance_address)).into());
			}
		} else {
			return Err(anyhow!("Invalid transaction type"));
		}
		Ok(())
	}
}
