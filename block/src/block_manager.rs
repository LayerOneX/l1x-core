use crate::block_state::BlockState;
use account::account_state::AccountState;
use anyhow::{anyhow, Error};
use log::info;
use primitives::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use system::{
	account::Account,
	block::{Block, BlockSignPayload, BlockType},
	block_header::BlockHeader,
	transaction::Transaction,
	config::Config,
};
use util::generic::current_timestamp_in_secs;
use compile_time_config::{BLOCK_VERSION, config::SLOTS_PER_EPOCH};


/// Manages the creation of new blocks and provides methods for block-related operations.
pub struct BlockManager;

impl<'a> BlockManager {
	/// Creates a new instance of `BlockManager`.
	pub fn new() -> BlockManager {
		BlockManager {}
	}

	/// Generates a new block based on provided transactions and block proposer information.
	///
	/// # Arguments
	///
	/// * `transactions` - The transactions to be included in the new block.
	/// * `block_proposer` - Information about the block proposer.
	/// * `block_state` - The state of the block.
	/// * `account_state` - The state of the accounts.
	///
	/// # Returns
	///
	/// A tuple containing the new block and its header if successful, otherwise returns an error.
	pub async fn create_regular_block(
		&self,
		transactions: Vec<Transaction>,
		cluster_address: Address,
		block_state: &BlockState<'a>,
		account_state: &AccountState<'a>,
	) -> Result<Block, Error> {
		self.create_block(transactions, cluster_address, block_state, account_state, BlockType::L1XTokenBlock).await
	}

	pub async fn create_system_block(&self,
		transactions: Vec<Transaction>,
		cluster_address: Address,
		block_state: &BlockState<'a>,
		account_state: &AccountState<'a>,
	) -> Result<Block, Error> {
		self.create_block(transactions, cluster_address, block_state, account_state, BlockType::SystemBlock).await
	}

	async fn create_block(
		&self,
		transactions: Vec<Transaction>,
		cluster_address: Address,
		block_state: &BlockState<'a>,
		account_state: &AccountState<'a>,
		block_type: BlockType,
	) -> Result<Block, Error> {
		let transactions = self.validate_nonce(transactions, account_state).await?;
		// Load the last block header
		let last_block_header =
			match block_state.block_head_header(cluster_address).await {
				Ok(lbh) => lbh,
				Err(_e) => BlockHeader::default(),
			};
		let timestamp = current_timestamp_in_secs()?;
		let block_number = last_block_header.block_number + 1;

		// Determine the current epoch
		let current_epoch = match self.calculate_current_epoch(block_number) {
				Ok(epoch) => epoch,
				Err(e) => {
					log::error!("Unable to get current_epoch: {:?}", e);
					last_block_header.epoch
				}
		};
		log::debug!("Current epoch in new block created {}", current_epoch);
		let new_block = create_block_internal(last_block_header, transactions, cluster_address, timestamp, current_epoch, block_type)?;
		log::debug!("New block created {}",new_block);
		Ok(new_block)
	} 

	/// Computes the hash of the given data using the SHA256 algorithm.
	///
	/// # Arguments
	///
	/// * `data` - The data to be hashed.
	///
	/// # Returns
	///
	/// The hash value.
	pub fn compute_block_hash(&self, data: &[u8]) -> BlockHash {
		let mut hasher = Sha256::new();
		hasher.update(data);
		let result = hasher.finalize();
		let hash_bytes = result.as_slice();
		let mut block_hash = BlockHash::default();
		block_hash.copy_from_slice(hash_bytes);
		block_hash
	}

	/// Validates the nonce sequence of transactions and returns only the valid ones.
	///
	/// # Arguments
	///
	/// * `transactions` - The transactions to be validated.
	/// * `account_state` - The state of the accounts.
	///
	/// # Returns
	///
	/// A vector containing only the valid transactions.
	pub async fn validate_nonce(
		&self,
		transactions: Vec<Transaction>,
		account_state: &AccountState<'a>,
	) -> Result<Vec<Transaction>, Error> {
		// Step 1: Sort transactions based on verifying_key
		let mut transactions_by_key: HashMap<VerifyingKeyBytes, Vec<Transaction>> = HashMap::new();
		for transaction in transactions {
			transactions_by_key
				.entry(transaction.verifying_key.clone())
				.or_default()
				.push(transaction);
		}

		// Step 2: Validate nonce sequence and keep only the correct ones
		let mut validated_transactions: Vec<Transaction> = Vec::new();
		for (verifying_key, mut txs) in transactions_by_key {
			txs.sort_by_key(|tx| tx.nonce);

			let mut prev_nonce =
				account_state.get_nonce(&Account::address(&verifying_key)?).await?;
			for tx in txs {
				if tx.nonce == prev_nonce + 1 {
					validated_transactions.push(tx.clone());
					prev_nonce = tx.nonce;
				}
				// If the nonce is not sequential, ignore this transaction
				// and assume that the next transactions with higher nonces are also invalid.
				else {
					break
				}
			}
		}

		Ok(validated_transactions)
	}
	
	pub fn calculate_current_epoch(&self, block_number: BlockNumber) -> Result<Epoch, Error> {
		if block_number == 0 {
			return Ok(0);
		}
		let current_epoch = block_number / SLOTS_PER_EPOCH;
		Ok(current_epoch as Epoch)
	}

	pub fn get_epoch_start_slot(&self, epoch: Epoch) -> Result<BlockNumber, Error> {
		Ok(epoch as BlockNumber * SLOTS_PER_EPOCH)
	}

	pub fn slots_elapsed_in_epoch(&self, block_number: BlockNumber) -> Result<BlockNumber, Error> {
		let current_epoch = self.calculate_current_epoch(block_number)?;
		let epoch_start_slot = self.get_epoch_start_slot(current_epoch)?;
		Ok(block_number - epoch_start_slot)
	}

	pub async fn get_epoch_duration(&self) -> Result<u64, Error> {
		let config = Config::get_config().await?;
		let slot_duration_seconds = config.block_time as u128;
		let epoch_duration_seconds = SLOTS_PER_EPOCH * slot_duration_seconds;
		Ok(epoch_duration_seconds as u64)
	}

	pub async fn get_epoch_duration_in_days(&self) -> Result<f64, Error> {
		let epoch_duration_seconds = self.get_epoch_duration().await?;
		let epoch_duration_days = epoch_duration_seconds as f64 / (60.0 * 60.0 * 24.0);
		Ok(epoch_duration_days)
	}

	// Helper method to determine if an epoch has ended 
	pub fn has_epoch_ended(&self, block_number: BlockNumber) -> Result<bool, anyhow::Error> {
		let current_epoch = self.calculate_current_epoch(block_number)?;
		let next_block_number = block_number + 1;
		let next_epoch = self.calculate_current_epoch(next_block_number)?;
		
		Ok(next_epoch > current_epoch)
	}

	pub async fn is_approaching_epoch_end(&self, current_block_number: BlockNumber, slots_remaining: u32) -> Result<bool, anyhow::Error> {
		let slots_elapsed = self.slots_elapsed_in_epoch(current_block_number)?;
		let threshold = SLOTS_PER_EPOCH / slots_remaining as u128; // 25% of slots remaining
		Ok(SLOTS_PER_EPOCH - slots_elapsed <= threshold)
	}
}

fn create_block_internal(last_block_header: BlockHeader, transactions: Vec<Transaction>, cluster_address: Address, timestamp: u64, epoch: Epoch, block_type: BlockType) -> Result<Block, Error> {
	// Increment the block number from the last block header
	let block_number: BlockNumber = last_block_header.block_number + 1;
	info!(
			"🟪🟪🟪 Creating block #{} in cluster 0x{} 🟪🟪🟪",
			block_number,
			hex::encode(&cluster_address)
		);

	// Use the block hash from the last block as the parent hash
	let parent_hash = last_block_header.block_hash;
	// create block header
	let new_block_header = BlockHeader {
		block_number,
		block_hash: [0; 32], 
		parent_hash,
		block_type,
		cluster_address,
		timestamp,
		num_transactions: i32::try_from((&transactions).len()).unwrap_or(i32::MAX),
		block_version: BLOCK_VERSION,
		state_hash: [0; 32], 
		epoch,
	};

	// create block
	let mut new_block = Block {
		block_header: new_block_header,
		transactions,
	};

	let new_block_sign_payload: BlockSignPayload = BlockSignPayload::from(&new_block);
	// create hash
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
	let block_hash = block_manager.compute_block_hash(&new_block_sign_payload_bytes);
	new_block.block_header.block_hash = block_hash;
	return Ok(new_block)
}
  

#[cfg(test)]
mod tests {
	use system::transaction::TransactionType;
	use super::*;

	#[test]
	fn test_create_block() {
		// create block header
		let last_block_header = BlockHeader {
			block_number: 0,
			block_hash: [0; 32],
			parent_hash: [0; 32],
			block_type: BlockType::L1XTokenBlock,
			cluster_address: [0; 20],
			timestamp: 0,
			num_transactions: 0,
			block_version: BLOCK_VERSION as u32,
			state_hash: [0; 32], 
			epoch: 0,
		};
		let tx1 = Transaction {
			nonce: 1,
			transaction_type: TransactionType::NativeTokenTransfer([1; 20], 10),
			fee_limit: 10000,
			signature: vec![],
			verifying_key: vec![],
			eth_original_transaction: None,
		};
		let tx2 = Transaction {
			nonce: 2,
			transaction_type: TransactionType::NativeTokenTransfer([2; 20], 20),
			fee_limit: 10000,
			signature: vec![],
			verifying_key: vec![],
			eth_original_transaction: None,
		};

		let transactions = vec![tx1, tx2];

		let block_proposer = BlockProposer {
			cluster_address: [0; 20],
			epoch: 0,
			address: [1; 20],
		};
		let timestamp = current_timestamp_in_secs().unwrap();
		// create block header
		let new_block_header = BlockHeader {
			block_number: last_block_header.block_number + 1,
			block_hash: [0; 32],
			parent_hash: last_block_header.block_hash,
			block_type: BlockType::L1XTokenBlock,
			cluster_address: block_proposer.cluster_address,
			timestamp,
			num_transactions: i32::try_from((&transactions).len()).unwrap_or(i32::MAX),
			block_version: BLOCK_VERSION,
			state_hash: [0; 32], 
			epoch: 0,
		};

		// create block
		let new_block = Block {
			block_header: new_block_header,
			transactions: transactions.clone(),
		};
		let new_block_sign_payload: BlockSignPayload = BlockSignPayload::from(&new_block);
		let new_block_sign_payload_bytes: Vec<u8> = bincode::serialize(&new_block_sign_payload).unwrap();

		let block_manager = BlockManager::new();
		let block_hash = block_manager.compute_block_hash(&new_block_sign_payload_bytes); // Implement this function to compute the block hash

		// Call the function
		let result = create_block_internal(last_block_header.clone(), transactions.clone(), block_proposer.cluster_address, timestamp, 0, BlockType::L1XTokenBlock);

		// Check if the result is Ok or Err
		match result {
			Ok(new_block) => {
				// Check properties of new_block and new_block_header
				assert_eq!(new_block.block_header.block_number, last_block_header.block_number + 1);
				assert_eq!(new_block.block_header.parent_hash, last_block_header.block_hash);
				assert_eq!(new_block.block_header.block_hash, block_hash);
				assert_eq!(new_block.block_header.block_type, last_block_header.block_type);
				assert_eq!(new_block.block_header.cluster_address, block_proposer.cluster_address);
				assert_eq!(new_block.block_header.timestamp, timestamp);
				assert_eq!(new_block.block_header.num_transactions, 2);
				assert_eq!(new_block.transactions, transactions);
			}
			Err(error) => {
				// Handle the error
				panic!("Test failed with error: {:?}", error);
			}
		}
	}
}
