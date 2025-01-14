use crate::validate_common::{ContractValidator, ValidateCommon};

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use contract::contract_state::ContractState;
use contract_instance::contract_instance_state::ContractInstanceState;
use db::db::DbTxConn;
use l1x_ebpf_runtime::verify_ebpf;
use primitives::*;
use system::{access::AccessType, transaction::Transaction};

pub struct ValidateContract;

#[async_trait]
impl<'a> ContractValidator<'a> for ValidateContract {
	async fn validate_contract_deployment(
		&self,
		transaction: &Transaction,
		contract_code: &ContractCode,
		sender: &Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		ValidateCommon::validate_common(transaction, sender, db_pool_conn).await?;
		verify_ebpf(&contract_code)?;
		Ok(())
	}

	async fn validate_contract_init(
		&self,
		transaction: &Transaction,
		contract_address: &Address,
		_arguments: &ContractArgument,
		sender: &Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		ValidateCommon::validate_common(transaction, sender, db_pool_conn).await?;
		
		let contract_state = ContractState::new(db_pool_conn).await?;
		// Here two things are validated:
		// 1. Whether this `contract_address` is present (is correct)
		// 2. Whether `sender` has permissions to initialize the contract
		let (access, contract_owner) = contract_state.get_contract_owner(contract_address).await?;
		if access == (AccessType::PRIVATE as i8) && contract_owner != *sender {
			return Err(anyhow!("Only owner can instantiate the PRIVATE contract code"))
		}
		Ok(())
	}

	async fn validate_contract_function_call(
		&self,
		transaction: &Transaction,
		contract_instance_address: &Address,
		_function_name: &ContractFunction,
		_arguments: &ContractArgument,
		sender: &Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		ValidateCommon::validate_common(transaction, sender, db_pool_conn).await?;

		let contract_instance_state = ContractInstanceState::new(db_pool_conn).await?;
		if !contract_instance_state.is_valid_contract_instance(contract_instance_address).await? {
			Err(anyhow!("Can't find the contract address: {}", hex::encode(&contract_instance_address)))?
		}
		Ok(())
	}
}
