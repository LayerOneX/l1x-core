use crate::{
	validate_contract_evm, validate_contract_l1xvm, validate_contract_xtalk,
	validate_staking::ValidateStaking, validate_token::ValidateToken,
};
use account::account_state::AccountState;
use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db::db::DbTxConn;
use log::{debug, info};
use primitives::*;
use system::{
	account::Account,
	contract::ContractType,
	transaction::{Transaction, TransactionType},
};

pub struct ValidateCommon {}

impl<'a> ValidateCommon {
	pub async fn validate_common(
		transaction: &Transaction,
		sender: &Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let account_state = AccountState::new(db_pool_conn).await?;
		let transaction_hash_string = hex::encode(transaction.transaction_hash()?);
		// Check if the transaction signature is valid
		if !transaction.verify_signature()? {
			info!("VALIDATE_COMMON => Invalid transaction signature {:?}, tx hash {}", transaction, transaction_hash_string);
			return Err(anyhow!("Invalid transaction signature"))
		}

		if !account_state.is_valid_account(sender).await? {
			info!("VALIDATE_COMMON => Invalid transaction sender {:?}, tx hash {}", hex::encode(sender), transaction_hash_string);
			return Err(anyhow!("Invalid transaction sender"))
		}

		let account = account_state.get_account(sender).await?;

		let expected_nonce = account.nonce + 1;
		if transaction.nonce != expected_nonce {
			info!(
				"\nVALIDATE_COMMON => Invalid nonce. Received {:?} != expected {:?}, tx hash {}",
				transaction.nonce, expected_nonce, transaction_hash_string
			);
			return Err(anyhow!("Invalid nonce"))
		}

		let required_balance = transaction.required_balance()?;
		if account.balance < required_balance {
			info!("VALIDATE_COMMON: Not enough balance to execute TX, required balance: {}, {} balance: {}, tx hash: {}", 
				required_balance, hex::encode(sender), account.balance, transaction_hash_string);
			return Err(anyhow!("Not enough balance"))
		}

		Ok(())
	}

	pub async fn validate_common_native_token_tx(
		transaction: &Transaction,
		sender: &Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<Account, Error> {
		let account_state = AccountState::new(db_pool_conn).await?;
		let transaction_hash_string = hex::encode(transaction.transaction_hash()?);
		// Check if the transaction signature is valid
		if !transaction.verify_signature()? {
			info!("VALIDATE_COMMON => Invalid transaction signature {:?}, tx hash {}", transaction, transaction_hash_string);
			return Err(anyhow!("Invalid transaction signature"))
		}

		if !account_state.is_valid_account(sender).await? {
			info!("VALIDATE_COMMON => Invalid transaction sender {}, tx hash {}", hex::encode(sender), transaction_hash_string);
			return Err(anyhow!("Invalid transaction sender"))
		}

		let account = account_state.get_account(sender).await?;

		if transaction.nonce <= account.nonce {
			info!(
				"\nVALIDATE_COMMON => Invalid nonce {:?} > {:?}, tx hash {}",
				transaction.nonce, account.nonce, transaction_hash_string
			);
			return Err(anyhow!("Invalid nonce"))
		}

		Ok(account)
	}

	pub async fn validate_tx(
		transaction: &Transaction,
		sender: &Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		debug!(
			"VALIDATE_COMMON => tx hash: {}",
			hex::encode(transaction.transaction_hash()?)
		);
		let contract_type = if transaction.eth_original_transaction.is_some() {
			ContractType::EVM
		} else {
			ContractType::L1XVM
		};

		let _ = match &transaction.transaction_type {
			TransactionType::NativeTokenTransfer(recipient, amount) => {
				util::generic::wb_list_check(sender, &recipient).await?;
				ValidateToken::validate_native_token(&transaction, *amount, &sender, db_pool_conn)
					.await?
			},
			TransactionType::SmartContractDeployment {
				access_type: _,
				contract_type,
				contract_code,
				deposit: _,
				salt: _,
			} => {
				let validator: &(dyn ContractValidator + Sync) = match contract_type {
					ContractType::L1XVM =>
						&validate_contract_l1xvm::ValidateContract
							as &(dyn ContractValidator + Sync),
					ContractType::EVM =>
						&validate_contract_evm::ValidateContract as &(dyn ContractValidator + Sync),
					ContractType::XTALK =>
						&validate_contract_xtalk::ValidateContract
							as &(dyn ContractValidator + Sync),
				};
				validator
					.validate_contract_deployment(transaction, contract_code, sender, db_pool_conn)
					.await?
			},
			TransactionType::SmartContractInit {
				contract_code_address,
				arguments,
				deposit: _,
			 } => {
				let validator: &(dyn ContractValidator + Sync) = match contract_type {
					ContractType::L1XVM =>
						&validate_contract_l1xvm::ValidateContract
							as &(dyn ContractValidator + Sync),
					ContractType::EVM =>
						&validate_contract_evm::ValidateContract as &(dyn ContractValidator + Sync),
					ContractType::XTALK =>
						&validate_contract_xtalk::ValidateContract
							as &(dyn ContractValidator + Sync),
				};
				validator
					.validate_contract_init(
						transaction,
						contract_code_address,
						arguments,
						sender,
						db_pool_conn,
					)
					.await?
			},
			TransactionType::SmartContractFunctionCall {
				contract_instance_address,
				function,
				arguments,
				deposit: _
			} => {
				let validator: &(dyn ContractValidator + Sync) = match contract_type {
					ContractType::L1XVM =>
						&validate_contract_l1xvm::ValidateContract
							as &(dyn ContractValidator + Sync),
					ContractType::EVM =>
						&validate_contract_evm::ValidateContract as &(dyn ContractValidator + Sync),
					ContractType::XTALK =>
						&validate_contract_xtalk::ValidateContract
							as &(dyn ContractValidator + Sync),
				};
				validator
					.validate_contract_function_call(
						transaction,
						&contract_instance_address,
						&function,
						&arguments,
						sender,
						db_pool_conn,
					)
					.await?
			},
			TransactionType::CreateStakingPool {
				contract_instance_address: _,
				min_stake: _,
				max_stake: _,
				min_pool_balance: _,
				max_pool_balance: _,
				staking_period: _,
			} => {},
			TransactionType::Stake { pool_address, amount } =>
				ValidateStaking::validate_stake(sender, &pool_address, *amount, db_pool_conn).await?,
			TransactionType::UnStake { pool_address, amount } =>
				ValidateStaking::validate_unstake(sender, &pool_address, *amount, db_pool_conn)
					.await?,
			TransactionType::StakingPoolContract {
				pool_address: _,
				contract_instance_address: _,
			} => {},
		};
		Ok(())
	}
}

#[async_trait]
pub(crate) trait ContractValidator<'a> {
	async fn validate_contract_deployment(
		&self,
		_transaction: &Transaction,
		_contract_code: &ContractCode,
		_sender: &Address,
		_db_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		Err(anyhow!("Deployment is not implemented"))
	}

	async fn validate_contract_init(
		&self,
		_transaction: &Transaction,
		_contract_address: &Address,
		_arguments: &ContractArgument,
		_sender: &Address,
		_db_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		Err(anyhow!("Initialization is not implemented"))
	}

	async fn validate_contract_function_call(
		&self,
		_transaction: &Transaction,
		_contract_instance_address: &Address,
		_function_name: &ContractFunction,
		_arguments: &ContractArgument,
		_sender: &Address,
		_db_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		Err(anyhow!("Smart contract function call is not implemented"))
	}
}
