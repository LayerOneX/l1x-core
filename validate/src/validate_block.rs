use std::collections::HashSet;
use std::sync::Arc;
use crate::validate_common::ValidateCommon;
use anyhow::{anyhow, Error};
use account::account_state::AccountState;
use block::block_manager::BlockManager;
use block::block_state::BlockState;
use block_proposer::block_proposer_manager::BlockProposerManager;
use compile_time_config::SYSTEM_REWARDS_DISTRIBUTOR;
use db::db::DbTxConn;
use primitives::{Address, Balance, BlockHash};
use runtime_config::{RuntimeConfigCache, RuntimeStakingInfoCache};
use system::{account::Account, block::BlockPayload, block_proposer::BlockProposer};
use system::block::{Block, BlockSignPayload};
use system::block_header::BlockHeader;
use system::transaction::{Transaction, TransactionType};
use execute::execute_fee;

pub struct ValidateBlock {}

impl<'a> ValidateBlock {
    pub async fn validate_block(
        block_payload: &BlockPayload,
        db_pool_conn: &'a DbTxConn<'a>,
		cluster_address: Address,
        proposer_address: Address,
    ) -> Result<(), Error> {
        // Verify the block signature
		block_payload.verify_signature().await.map_err(|error| anyhow!("Signature is invalid: {:?}", error))?;

        // Verify the block proposer address matches the expected proposer address
        let proposer_address_from_key = Account::address(&block_payload.verifying_key)?;
        if proposer_address_from_key != proposer_address {
            return Err(Error::msg("Block proposer address mismatch"));
        }

        // Additional validity checks
        let block_state = BlockState::new(&db_pool_conn).await?;
        let last_block_header = block_state.block_head_header(cluster_address).await?;
		// Validate block header and transactions
		validate_block_header_and_transactions(&block_payload, db_pool_conn, cluster_address, None).await?;

        Ok(())
    }

	pub async fn validate_proposed_block(
		block_payload: &BlockPayload,
		db_pool_conn: &'a DbTxConn<'a>,
		cluster_address: Address,
		last_block_votes_validators: HashSet<Address>,
	) -> Result<(), Error> {
		let block_proposer_address = Account::address(&block_payload.verifying_key)?;
		let block_proposer_manager = BlockProposerManager {};
		let block_proposer = BlockProposer::new(
			block_payload.block.block_header.cluster_address,
			block_payload.block.block_header.epoch,
			block_proposer_address,
		);
		let block_number = block_payload.block.block_header.block_number;

		if block_payload.block.is_system_block() {
			return Err(anyhow!("System type block #{} can't be proposed", block_number));
		}

		if !block_proposer_manager
			.is_block_proposer(
				block_proposer.clone(),
				db_pool_conn,
			)
			.await? {
			return Err(anyhow!("Block #{} is not received from authorized block proposer: {}", block_number, hex::encode(&block_proposer.address)));
		}

		// Verify the signature of the block to know definitively which node sent it
		block_payload.verify_signature().await.map_err(|error| anyhow!("Signature is invalid: {:?}", error))?;

		// Validate rewards for the previous block can be distributed or not
		let eligible_validators_for_reward =
			get_eligible_validators_for_reward(last_block_votes_validators, db_pool_conn).await?;
		// Validate block header and transactions
		validate_block_header_and_transactions(&block_payload, db_pool_conn, cluster_address, Some(eligible_validators_for_reward)).await?;

		Ok(())
	}
}

async fn validate_block_header_and_transactions<'a>(
	block_payload: &BlockPayload,
	db_pool_conn: &'a DbTxConn<'a>,
	cluster_address: Address,
	mut eligible_validators_for_reward: Option<HashSet<Address>>,
) -> Result<(), Error> {
	let block_state = BlockState::new(db_pool_conn).await?;
	let last_block_header = block_state.block_head_header(cluster_address).await?;
	let rt_stake_info = RuntimeStakingInfoCache::get().await?;
	let rt_config = RuntimeConfigCache::get().await?;
	// Validate block header
	if last_block_header != block_payload.block.block_header {
		validate_expected_block_fields(&last_block_header, &block_payload.block)?;
	}
	validate_expected_block_hash(&block_payload)?;
	// Validate transactions
	for tx in &block_payload.block.transactions {
		let sender = Account::address(&tx.verifying_key)?;

		if sender == SYSTEM_REWARDS_DISTRIBUTOR {
			if let Some(ref mut validators) = eligible_validators_for_reward {
				let receiver = validate_reward_tx(tx, rt_config.clone())?;
				let validator = rt_stake_info
					.reward_node_map
					.get(&receiver)
					.ok_or(anyhow!("Reward receiver is not a validator"))?;
				if !validators.remove(validator) {
					return Err(
						anyhow!(
							"Invalid transaction, unknown validator for reward: block_number: {:?}",
							last_block_header.block_number
						)
					);
				}
			}
		} else {
			ValidateCommon::validate_tx(tx, &sender, db_pool_conn).await?;
		}
	}

	// Ensure all validators are getting rewards
	if let Some(validators) = eligible_validators_for_reward {
		if !validators.is_empty() {
			return Err(anyhow!(
                "Invalid reward distribution: No. of remaining validators: {:?}", validators.len()
            ));
		}
	}

	Ok(())
}

async fn get_eligible_validators_for_reward<'a>(
	last_block_votes_validators: HashSet<Address>,
	db_pool_conn: &'a DbTxConn<'a>,
) -> Result<HashSet<Address>, Error> {
	if !last_block_votes_validators.is_empty() {
		let rt_config = RuntimeConfigCache::get().await?;
		let dummy_tx = Transaction::new_system(
			&Default::default(),
			Default::default(),
			TransactionType::NativeTokenTransfer(Address::default(), Balance::default()),
			Default::default(),
		);
		let mut execution_fee = execute_fee::ExecuteFee::new(&dummy_tx, execute_fee::SystemFeeConfig::new());

		let fee_limit = execution_fee.get_token_transfer_tx_total_fee()?;
		let reward_amount = rt_config.rewards.validated_block_reward;

		// Calculate total rewards and fees
		let total_votes = last_block_votes_validators.len() as u128;
		let total_rewards = reward_amount.0.checked_mul(total_votes)
			.ok_or_else(|| anyhow::anyhow!("Overflow while calculating total rewards"))?;
		let total_fees = fee_limit.checked_mul(total_votes)
			.ok_or_else(|| anyhow::anyhow!("Overflow while calculating total fees"))?;
		let total_cost = total_rewards.checked_add(total_fees)
			.ok_or_else(|| anyhow::anyhow!("Overflow while calculating total cost"))?;

		let account_state = AccountState::new(&db_pool_conn).await?;
		let balance = account_state.get_balance(&SYSTEM_REWARDS_DISTRIBUTOR).await.unwrap_or(0);

		if balance < total_cost {
			return Ok(HashSet::new());
		}
		return Ok(last_block_votes_validators);
	} else {
		Ok(HashSet::new())
	}
}

fn validate_expected_block_fields(
	last_block_header: &BlockHeader,
	new_block: &Block,
) -> Result<(), Error> {
	// Validate block number
	let expected_block_number = last_block_header.block_number.checked_add(1)
		.ok_or(anyhow!("Error incrementing block number. Potential overflow"))?;
	if expected_block_number != new_block.block_header.block_number {
		return Err(anyhow!(
		"Incorrect block number: actual {:?}, expected: {:?}",
		new_block.block_header.block_number,
		expected_block_number
		));
	}

	// Validate parent hash
	let expected_parent_hash = last_block_header.block_hash;
	if expected_parent_hash != new_block.block_header.parent_hash {
		return Err(anyhow!(
		"Incorrect parent hash: actual {:?}, expected: {:?}",
		hex::encode(new_block.block_header.parent_hash),
		hex::encode(expected_parent_hash)
		));
	}

	// Validate cluster_address
	let expected_cluster_address = last_block_header.cluster_address;
	if expected_cluster_address != new_block.block_header.cluster_address {
		return Err(anyhow!(
		"Incorrect cluster address: actual {:?}, expected: {:?}",
		hex::encode(new_block.block_header.cluster_address),
		hex::encode(expected_cluster_address)
		));
	}

	// Validate timestamp
	if last_block_header.timestamp > new_block.block_header.timestamp {
		return Err(anyhow!(
		"Incorrect timestamp: last_block_timestamp {:?}, new_block_timestamp: {:?}",
		last_block_header.timestamp,
		new_block.block_header.timestamp
		));
	}

	// Validate no. of transactions
	if new_block.transactions.len() != new_block.block_header.num_transactions as usize {
		return Err(anyhow!(
		"Incorrect number of transactions: actual {:?}, expected: {:?}",
		new_block.block_header.num_transactions,
		new_block.transactions.len()
		));
	}
	Ok(())
}

fn validate_expected_block_hash(block_payload: &BlockPayload) -> Result<(), Error> {
	let expected_block_hash = compute_block_hash(block_payload)?;
	if expected_block_hash != block_payload.block.block_header.block_hash {
		return Err(anyhow!(
		"Incorrect block hash: actual {:?}, expected: {:?}",
		hex::encode(block_payload.block.block_header.block_hash),
		hex::encode(expected_block_hash)
	));
	}
	Ok(())
}

fn validate_reward_tx(
	transaction: &Transaction,
	rt_config: Arc<RuntimeConfigCache>,
) -> Result<Address, Error> {
	let reward_amount = rt_config.rewards.validated_block_reward.0;
	if reward_amount == 0 {
		return Err(anyhow!("Invalid transaction: No amount configured for reward distribution in RuntimeConfig"));
	}

	match &transaction.transaction_type {
		TransactionType::NativeTokenTransfer(receiver, amount) => {
			if reward_amount != *amount {
				return Err(anyhow!(
                    "Incorrect reward amount, actual: {}, expected: {}",
                    amount,
                    reward_amount
                ));
			}
			Ok(*receiver)
		}
		_ => Err(anyhow!(
            "Invalid transaction type: Expected NativeTokenTransfer"
        )),
	}
}

fn compute_block_hash(block_payload: &BlockPayload) -> Result<BlockHash, Error> {
	let new_block_sign_payload: BlockSignPayload = block_payload.into();
	let new_block_sign_payload_bytes: Vec<u8> =
		match bincode::serialize(&new_block_sign_payload) {
			Ok(val) => val,
			Err(err) =>
				return Err(anyhow!(
						"Error converting new_block_sign_payload to bytes: {:?}",
						err
					)),
		};
	let block_manager = BlockManager::new();
	Ok(block_manager.compute_block_hash(&new_block_sign_payload_bytes))
}

#[cfg(test)]
mod tests {
	use system::block::{Block, BlockType};
	use super::*;

	#[test]
	fn test_validate_new_block_fields() {
		let last_block_header = BlockHeader::default();
		let new_block_header = BlockHeader {
			block_number: last_block_header.block_number + 1,
			block_hash: [0; 32],
			parent_hash: last_block_header.block_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: last_block_header.cluster_address,
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: last_block_header.parent_hash,
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};
		let block_payload = BlockPayload {
			block,
			signature: vec![],
			verifying_key: vec![],
			sender: [0; 20],
		};
		let result = validate_expected_block_fields(&last_block_header, &block_payload.block);
		assert!(result.is_ok());
	}

	#[test]
	fn test_validate_new_block_fields_for_incorrect_block_number() {
		let last_block_header = BlockHeader::default();
		let new_block_number = last_block_header.block_number + 2;
		let parent_hash = last_block_header.block_hash;

		let new_block_header = BlockHeader {
			block_number: new_block_number,
			block_hash: [0; 32],
			parent_hash: last_block_header.block_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: last_block_header.cluster_address,
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: last_block_header.parent_hash,
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let block_payload = BlockPayload {
			block,
			signature: vec![],
			verifying_key: vec![],
			sender: [0; 20],
		};
		let result = validate_expected_block_fields(&last_block_header, &block_payload.block);
		assert!(result.is_err());
		if let Err(error) = result {
			assert!(error.to_string().contains("Incorrect block number"));
		}
	}

	#[test]
	fn test_validate_new_block_fields_for_incorrect_parent_hash() {
		let last_block_header = BlockHeader::default();
		let new_block_number = last_block_header.block_number + 1;
		let parent_hash = last_block_header.block_hash;

		let new_block_header = BlockHeader {
			block_number: new_block_number,
			block_hash: [1; 32],
			parent_hash: [2; 32],
			block_type: BlockType::L1XTokenBlock,
			cluster_address: last_block_header.cluster_address,
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: last_block_header.parent_hash,
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let block_payload = BlockPayload {
			block,
			signature: vec![],
			verifying_key: vec![],
			sender: [0; 20],
		};
		let result = validate_expected_block_fields(&last_block_header, &block_payload.block);
		assert!(result.is_err());
		if let Err(error) = result {
			assert!(error.to_string().contains("Incorrect parent hash"));
		}
	}

	#[test]
	fn test_validate_new_block_fields_for_incorrect_cluster_address() {
		let last_block_header = BlockHeader::default();
		let new_block_number = last_block_header.block_number + 1;
		let parent_hash = last_block_header.block_hash;

		let new_block_header = BlockHeader {
			block_number: new_block_number,
			block_hash: [1; 32],
			parent_hash: last_block_header.block_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: [1; 20],
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: last_block_header.parent_hash,
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let block_payload = BlockPayload {
			block,
			signature: vec![],
			verifying_key: vec![],
			sender: [0; 20],
		};
		let result = validate_expected_block_fields(&last_block_header, &block_payload.block);
		assert!(result.is_err());
		if let Err(error) = result {
			assert!(error.to_string().contains("Incorrect cluster address"));
		}
	}

	#[test]
	fn test_validate_new_block_fields_for_incorrect_timestamp() {
		let mut last_block_header = BlockHeader::default();
		last_block_header.timestamp = 1;

		let new_block_number = last_block_header.block_number + 1;

		let new_block_header = BlockHeader {
			block_number: new_block_number,
			block_hash: [1; 32],
			parent_hash: last_block_header.block_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: last_block_header.cluster_address,
			timestamp: 0,
			num_transactions: 0,
			block_version: 1,
			state_hash: last_block_header.parent_hash,
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let block_payload = BlockPayload {
			block,
			signature: vec![],
			verifying_key: vec![],
			sender: [0; 20],
		};
		let result = validate_expected_block_fields(&last_block_header, &block_payload.block);
		assert!(result.is_err());
		if let Err(error) = result {
			assert!(error.to_string().contains("Incorrect timestamp"));
		}
	}

	#[test]
	fn test_validate_new_block_fields_for_incorrect_num_transaction() {
		let last_block_header = BlockHeader::default();
		let new_block_number = last_block_header.block_number + 1;

		let new_block_header = BlockHeader {
			block_number: new_block_number,
			block_hash: [1; 32],
			parent_hash: last_block_header.block_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: last_block_header.cluster_address,
			timestamp: 1,
			num_transactions: 1,
			block_version: 1,
			state_hash: last_block_header.parent_hash,
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let block_payload = BlockPayload {
			block,
			signature: vec![],
			verifying_key: vec![],
			sender: [0; 20],
		};
		let result = validate_expected_block_fields(&last_block_header, &block_payload.block);
		assert!(result.is_err());
		if let Err(error) = result {
			assert!(error.to_string().contains("Incorrect number of transactions"));
		}
	}

	#[test]
	fn test_validate_block_hash_for_correct_hash() {
		let last_block_header = BlockHeader::default();
		let new_block_number = last_block_header.block_number + 1;
		let parent_hash = last_block_header.block_hash;
		let new_block_sign_payload = BlockSignPayload {
			block_number: new_block_number,
			parent_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: last_block_header.cluster_address,
			transactions: vec![],
			timestamp: 1,
			num_transactions: 0,
			block_version: last_block_header.block_version,
 			state_hash: last_block_header.state_hash,
			epoch: 1,
		};
		let new_block_sign_payload_bytes: Vec<u8> = bincode::serialize(&new_block_sign_payload).unwrap();
		let block_manager = BlockManager::new();
		let block_hash = block_manager.compute_block_hash(&new_block_sign_payload_bytes);
		let new_block_header = BlockHeader {
			block_number: new_block_number,
			block_hash,
			parent_hash: last_block_header.block_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: last_block_header.cluster_address,
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: last_block_header.parent_hash,
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let block_payload = BlockPayload {
			block,
			signature: vec![],
			verifying_key: vec![],
			sender: [0; 20],
		};
		assert!(validate_expected_block_hash(&block_payload).is_ok());
	}

	#[test]
	fn test_validate_block_hash_for_incorrect_hash() {
		let last_block_header = BlockHeader::default();
		let new_block_number = last_block_header.block_number + 1;
		let parent_hash = last_block_header.block_hash;

		let new_block_header = BlockHeader {
			block_number: new_block_number,
			block_hash: [0; 32],
			parent_hash: last_block_header.block_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: last_block_header.cluster_address,
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: last_block_header.parent_hash,
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let block_payload = BlockPayload {
			block,
			signature: vec![],
			verifying_key: vec![],
			sender: [0; 20],
		};
		let result = validate_expected_block_hash(&block_payload);
		assert!(result.is_err());
		if let Err(error) = result {
			assert!(error.to_string().contains("Incorrect block hash"));
		}
	}
}
