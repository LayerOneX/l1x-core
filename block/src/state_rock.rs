use anyhow::{anyhow, Error};
use async_trait::async_trait;
use chrono::Duration;
use db::utils::ToByteArray;
use db_traits::{base::BaseState, block::BlockState};
use l1x_rpc::{rpc_model, rpc_model::TransactionResponse};
use primitives::{arithmetic::ScalarBig, Address, BlockHash, BlockNumber, Epoch, TransactionHash, TransactionSequence};
use rocksdb::{WriteBatch, DB};

use std::fs::remove_dir_all;
use system::{
	account::Account,
	block::{Block, BlockResponse},
	block_header::BlockHeader,
	block_meta_info::BlockMetaInfo,
	chain_state::ChainState,
	transaction::Transaction,
	transaction_receipt::TransactionReceiptResponse,
};
use system::transaction::TransactionMetadata;
use util::{convert::to_proto_transaction, generic::{current_timestamp_in_micros, current_timestamp_in_millis}};

pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}

impl StateRock {
	fn add_transaction_to_address(
		&self,
		from_address: &Address,
		tx_hash: &TransactionHash,
	) -> Result<(), Error> {
		let key = format!("from_address_to_tx_hashes:{:?}", from_address);
		let mut tx_hashes: Vec<TransactionHash> = match self.db.get(&key)? {
			Some(value) => serde_json::from_slice(&value)?,
			None => Vec::new(),
		};

		tx_hashes.push(*tx_hash);

		let value = serde_json::to_vec(&tx_hashes)?;
		self.db.put(&key, value)?;

		Ok(())
	}

	fn get_transactions_for_address(
		&self,
		from_address: &Address,
	) -> Result<Vec<TransactionHash>, Error> {
		let key = format!("from_address_to_tx_hashes:{:?}", from_address);
		match self.db.get(&key)? {
			Some(value) => {
				let tx_hashes: Vec<TransactionHash> = serde_json::from_slice(&value)?;
				Ok(tx_hashes)
			},
			None => Ok(Vec::new()),
		}
	}

	pub fn get_all_block_headers(
		&self,
		lower_block_number: BlockNumber,
	) -> Result<Vec<BlockHeader>, Error> {
		let start_key = format!("block_header:{:016x}", lower_block_number);
		let iter = self.db.iterator(rocksdb::IteratorMode::From(
			start_key.as_bytes(),
			rocksdb::Direction::Forward,
		));

		let mut block_headers = Vec::new();

		for item in iter {
			match item {
				Ok((key, value)) => {
					if let Ok(key_str) = String::from_utf8(key.to_vec()) {
						if let Some(block_num_str) = key_str.strip_prefix("block_header:") {
							if let Ok(block_num) = u128::from_str_radix(block_num_str, 16) {
								if block_num > lower_block_number {
									let block_header: BlockHeader =
										serde_json::from_slice(&value).map_err(Error::new)?;
									block_headers.push(block_header);
								} else {
									break; // Stop if block number is no longer greater
								}
							}
						}
					}
				},
				Err(e) => return Err(Error::new(e)),
			}
		}

		Ok(block_headers)
	}

	pub fn store_transaction_for_block_number(
		&self,
		block_number: BlockNumber,
		transaction_hash: TransactionHash,
	) -> Result<(), Error> {
		let key = format!("block_vec_tx_hashes:{:016x}", block_number); // Added prefix 'block_tx'
		let mut hashes = match self.db.get(&key)? {
			Some(data) => serde_json::from_slice(&data)?, // Deserialize existing data
			None => Vec::new(),
		};

		hashes.push(transaction_hash);
		let serialized_hashes = serde_json::to_vec(&hashes)?; // Serialize the updated list
		self.db.put(&key, serialized_hashes)?;

		Ok(())
	}

	fn get_transactions_for_block(
		&self,
		block_number: BlockNumber,
	) -> Result<Vec<TransactionHash>, Error> {
		let key = format!("block_vec_tx_hashes:{:016x}", block_number);
		match self.db.get(key)? {
			Some(value) => {
				let tx_hashes: Vec<TransactionHash> = serde_json::from_slice(&value)?;
				Ok(tx_hashes)
			},
			None => Ok(Vec::new()),
		}
	}
}

#[async_trait]
impl BaseState<Block> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, block: &Block) -> Result<(), Error> {
		let key = format!("block:{:?}", block.block_header.block_number);
		let value = serde_json::to_string(block)?;
		self.db.put(key, value)?;
		Ok(())
	}

	async fn update(&self, block: &Block) -> Result<(), Error> {
		let key = format!("block:{:?}", block.block_header.block_number);
		let value = serde_json::to_string(block)?;
		self.db.put(key, value)?;
		Ok(())
	}

	async fn raw_query(&self, _query: &str) -> Result<(), Error> {
		// Remove the database directory
		remove_dir_all(&self.db_path)?;

		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		Ok(())
	}
}

#[async_trait]
impl BlockState for StateRock {
	async fn store_block_head(
		&self,
		cluster_address: &Address,
		block_number: BlockNumber,
		block_hash: BlockHash,
	) -> Result<(), Error> {
		let block_head = ChainState { cluster_address: *cluster_address, block_number, block_hash };
		let key = format!("block_head:{:?}", cluster_address);
		let value = serde_json::to_string(&block_head)?;
		self.db.put(key, value)?;
		Ok(())
	}

	async fn update_block_head(
		&self,
		cluster_address: Address,
		block_number: BlockNumber,
		block_hash: BlockHash,
	) -> Result<(), Error> {
		let block_head = ChainState { cluster_address, block_number, block_hash };
		let key = format!("block_head:{:?}", cluster_address);
		let value = serde_json::to_string(&block_head)?;
		self.db.put(key, value)?;
		Ok(())
	}

	async fn load_chain_state(&self, cluster_address: Address) -> Result<ChainState, Error> {
		let key = format!("block_head:{:?}", cluster_address);
		match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				Ok(serde_json::from_str(&value_str)?)
			},
			None => Ok(ChainState { cluster_address, block_number: 0, block_hash: [0u8; 32] }),
		}
	}

	async fn block_head(&self, cluster_address: Address) -> Result<Block, Error> {
		let chain_state = self.load_chain_state(cluster_address).await?;
		self.load_block(chain_state.block_number, &cluster_address).await
	}

	async fn block_head_header(&self, cluster_address: Address) -> Result<BlockHeader, Error> {
		let chain_state = self.load_chain_state(cluster_address).await?;
		self.load_block_header(chain_state.block_number, &cluster_address).await
	}

	async fn store_block(&self, block: Block) -> Result<(), Error> {
		let block_meta_info = BlockMetaInfo {
			cluster_address: block.block_header.cluster_address,
			block_number: block.block_header.block_number,
			block_executed: false,
		};
		let key = format!("block_meta_info:{:?}", block.block_header.block_number);
		let value = serde_json::to_string(&block_meta_info)?;
		self.db.put(key, &value)?;

		self.store_block_header(block.block_header.clone()).await?;
		let transactions = block.transactions.clone();

		for (tx_sequence, transaction) in transactions.iter().enumerate() {
			self.store_transaction(
				block.block_header.block_number,
				block.block_header.block_hash,
				transaction.clone(),
				tx_sequence as TransactionSequence,
			)
			.await?;
		}

		self.update_block_head(
			block.block_header.cluster_address,
			block.block_header.block_number,
			block.block_header.block_hash,
		)
		.await?;

		Ok(())
	}

	async fn batch_store_blocks(&self, blocks: Vec<Block>) -> Result<(), Error> {
		let last_block = match blocks.last() {
			Some(block) => block.clone(),
			None => return Err(anyhow!("No blocks to store")),
		};

		// Create a batch statement
		// let mut blocks_batch: Batch = Default::default();
		let mut blocks_batch = WriteBatch::default();

		let mut block_headers: Vec<BlockHeader> = Vec::new();

		let mut transactions: Vec<(BlockNumber, BlockHash, Transaction)> = Vec::new();

		for block in blocks {
			let block_number = i64::try_from(block.block_header.block_number).unwrap_or(i64::MAX);

			log::info!(
				"ℹ️  Storing block #{} for cluster 0x{}",
				block_number,
				hex::encode(block.block_header.cluster_address),
			);

			self.store_block(block.clone()).await?;

			let block_meta_info = BlockMetaInfo {
				cluster_address: block.block_header.cluster_address,
				block_number: block.block_header.block_number,
				block_executed: false,
			};
			let key = format!("block_meta_info:{:?}", block.block_header.block_number);
			let value = serde_json::to_string(&block_meta_info)?;

			blocks_batch.put(key, value);

			// block_header
			block_headers.push(block.block_header.clone());

			let block_hash = block.block_header.block_hash;

			for transaction in block.transactions {
				transactions.push((block.block_header.block_number, block_hash, transaction));
			}
		}

		// Rocksdb Run the batch
		self.db.write(blocks_batch).unwrap();

		// Store block headers
		self.batch_store_block_headers(block_headers).await?;
		// Store transactions
		//println!("**** 55555 Block->StateRocks->batch_store_blocks
		// transactions:{:?}",transactions);

		self.batch_store_transactions(transactions).await?;

		// Update block_head
		self.update_block_head(
			last_block.block_header.cluster_address,
			last_block.block_header.block_number,
			last_block.block_header.block_hash,
		)
		.await?;
		//println!("**** 66666 Block->StateRocks->batch_store_blocks");
		Ok(())
	}

	async fn store_transaction(
		&self,
		block_number: BlockNumber,
		block_hash: BlockHash,
		transaction: Transaction,
		tx_sequence: TransactionSequence,
	) -> Result<(), Error> {
		let from_address = Account::address(&transaction.verifying_key)?;
		let timestamp = current_timestamp_in_millis()?;
		let fee_used = transaction.fee_limit;
		let tx_hash = transaction.transaction_hash()?;
		let key = format!("block_transaction:{:?}", tx_hash.clone());
		let value = serde_json::to_string(&transaction)?;
		self.db.put(key, value)?;
		let key = format!("tx_hash_to_block_hash:{:?}", tx_hash.clone());
		self.db.put(key, block_hash)?;
		let key = format!("tx_hash_to_block_number:{:?}", tx_hash.clone());
		self.db.put(key, block_number.to_be_bytes())?;
		let key = format!("tx_hash_to_fee_used:{:?}", tx_hash.clone());
		self.db.put(key, fee_used.to_be_bytes())?;
		let key = format!("tx_hash_to_from_address:{:?}", tx_hash.clone());
		self.db.put(key, from_address)?;
		let key = format!("tx_hash_to_timestamp:{:?}", tx_hash.clone());
		self.db.put(key, timestamp.to_be_bytes())?;
		let key = format!("tx_hash_to_tx_sequence:{:?}", tx_hash.clone());
		self.db.put(key, tx_sequence.to_be_bytes())?;
		self.add_transaction_to_address(&from_address, &tx_hash.clone())?;
		self.store_transaction_for_block_number(block_number, tx_hash)?;
		Ok(())
	}

	async fn batch_store_transactions(
		&self,
		transactions: Vec<(BlockNumber, BlockHash, Transaction)>,
	) -> Result<(), Error> {
		// Create a batch statement
		let mut batch = WriteBatch::default();

		let mut batch_values = Vec::new();
		for (block_number, block_hash, transaction) in transactions {
			let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
			let fee_used_bytes: ScalarBig =
				transaction.fee_limit.to_byte_array_le(ScalarBig::default());
			let transaction_bytes: Vec<u8> = bincode::serialize(&transaction)?;
			let transaction_hash = transaction.transaction_hash()?;
			let from_address = Account::address(&transaction.verifying_key)?;
			let timestamp = current_timestamp_in_millis()?;

			batch_values.push((
				transaction_hash,
				block_number,
				transaction_bytes,
				block_hash,
				fee_used_bytes,
				from_address,
				timestamp,
			));
			let key = format!("block_transaction:{:?}", transaction_hash);
			let value: String = serde_json::to_string(&batch_values)?;
			batch.put(key, value);
		}

		// Rocksdb Run the batch
		self.db.write(batch).unwrap();

		Ok(())
	}

	async fn store_transaction_metadata(&self, metadata: TransactionMetadata) -> Result<(), Error> {
		let key = format!("transaction_metadata:{:?}", metadata.transaction_hash);
		let value = serde_json::to_string(&metadata)?;
		self.db.put(key, value)?;
		Ok(())
	}

	async fn load_transaction_metadata(
		&self,
		tx_hash: TransactionHash,
	) -> Result<TransactionMetadata, Error> {
		let key = format!("transaction_metadata:{:?}", tx_hash);
		match self.db.get(&key)? {
			Some(v) => Ok(serde_json::from_slice(&v)?),
			None => Err(anyhow!("Can't load transaction_metadata, tx_hash: {}", hex::encode(tx_hash)))
		}
	}

	async fn get_transaction_count(
		&self,
		address: &Address,
		block_number: Option<BlockNumber>,
	) -> Result<u64, Error> {
		match block_number {
			Some(block_num) => Ok(self.get_transactions_for_block(block_num)?.len() as u64),
			None => Ok(self.get_transactions_for_address(address)?.len() as u64),
		}
	}

	async fn load_block(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<Block, Error> {
		let block_header: BlockHeader =
			self.load_block_header(block_number, cluster_address).await?;
		let transactions: Vec<Transaction> = self.load_transactions(block_number).await?;
		Ok(Block { block_header, transactions })
	}

	async fn get_block_number_by_hash(&self, block_hash: BlockHash) -> Result<BlockNumber, Error> {
		todo!()
	}

	async fn load_block_response(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<BlockResponse, Error> {
		let block_header: BlockHeader =
			self.load_block_header(block_number, cluster_address).await?;
		let transactions: Vec<TransactionReceiptResponse> =
			self.load_transactions_response(block_number).await?;
		Ok(BlockResponse { block_header, transactions })
	}

	async fn store_block_header(&self, block_header: BlockHeader) -> Result<(), Error> {
		let timestamp = current_timestamp_in_micros()?;
		let key = format!("block_header:{:?}", block_header.block_number);
		let value = serde_json::to_string(&block_header)?;
		self.db.put(key, value)?;
		let key = format!("block_number_to_timestamp:{:?}", block_header.block_number);
		self.db.put(key, timestamp.to_be_bytes())?;
		/*let key = format!("block_number_to_cluster_address:{:?}", block_header.block_number);
		self.db.put(key, &block_header.cluster_address)?;*/
		Ok(())
	}

	async fn batch_store_block_headers(
		&self,
		block_headers: Vec<BlockHeader>,
	) -> Result<(), Error> {
		// Create a batch statement
		let mut batch = WriteBatch::default();

		for block_header in block_headers {
			let block_number = i64::try_from(block_header.block_number).unwrap_or(i64::MAX);
			let block_hash = block_header.block_hash;
			let parent_hash = block_header.parent_hash;
			let timestamp = current_timestamp_in_micros()?; // Convert the duration to microseconds
			let microseconds_from_epoch = i64::try_from(timestamp).unwrap_or(i64::MAX);
			let timestamp = Duration::microseconds(microseconds_from_epoch);
			let _block_type: i8 = block_header.block_type.into();
			let num_transactions = block_header.num_transactions;

			let batch_values = BlockHeader {
				block_number: block_header.block_number,
				block_hash,
				parent_hash,
				cluster_address: block_header.cluster_address,
				block_type: block_header.block_type,
				timestamp: timestamp.num_microseconds().unwrap_or(i64::MAX) as u64,
				num_transactions,
				block_version: block_header.block_version as u32,
				state_hash: block_header.state_hash,
				epoch: block_header.epoch,
			};
			let key = format!("block_header:{:?}", block_number);
			let value = serde_json::to_string(&batch_values)?;
			batch.put(key, value);
		}

		// Rocksdb Run the batch
		self.db.write(batch).unwrap();

		Ok(())
	}

	async fn load_genesis_block_time(
		&self
	) -> Result<(u64, u64), Error> {
		//please holder, this has to be implemented
		todo!()
	}

	async fn store_genesis_block_time(
		&self,
		_block_number: BlockNumber,
		_epoch: Epoch,
	) -> Result<(), Error> {
		//please holder, this has to be implemented
		todo!()
	}

	async fn load_block_header(
		&self,
		block_number: BlockNumber,
		_cluster_address: &Address,
	) -> Result<BlockHeader, Error> {
		let key = format!("block_header:{:?}", block_number);
		match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				Ok(serde_json::from_str(&value_str)?)
			},
			None => Err(Error::msg("Account not found")),
		}
	}

	async fn load_transaction(
		&self,
		transaction_hash: TransactionHash,
	) -> Result<Option<Transaction>, Error> {
		let key = format!("block_transaction:{:?}", transaction_hash);

		match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				Ok(serde_json::from_str(&value_str)?)
			},
			None => Ok(None),
		}
	}

	async fn load_transaction_receipt(
		&self,
		transaction_hash: TransactionHash,
		_cluster_address: Address
	) -> Result<Option<TransactionReceiptResponse>, Error> {
		let key = format!("block_transaction:{:?}", transaction_hash.clone());
		let transaction = match self.db.get(key)? {
			Some(value) => {
				let value_str = String::from_utf8_lossy(&value);
				serde_json::from_str(&value_str)?
			},
			None => return Err(Error::msg("No transaction found")),
		};
		let key = format!("tx_hash_to_block_hash:{:?}", transaction_hash.clone());
		let block_hash: BlockHash = match self.db.get(key)? {
			Some(value) => value.try_into().map_err(|_| Error::msg("Invalid block hash length"))?,
			None => return Err(Error::msg("No block_hash found")),
		};
		let key = format!("tx_hash_to_block_number:{:?}", transaction_hash.clone());
		let block_number: BlockNumber = match self.db.get(key)? {
			Some(value) => {
				let bytes: [u8; 16] =
					value.try_into().map_err(|_| Error::msg("Invalid block number length"))?;
				BlockNumber::from_be_bytes(bytes)
			},
			None => return Err(Error::msg("No block_number found")),
		};

		let key = format!("tx_hash_to_fee_used:{:?}", transaction_hash.clone());
		let fee_used = match self.db.get(key)? {
			Some(value) => {
				let bytes: [u8; 16] =
					value.try_into().map_err(|_| Error::msg("Invalid fee used length"))?;
				BlockNumber::from_be_bytes(bytes)
			},
			None => return Err(Error::msg("No fee_used found")),
		};

		let key = format!("tx_hash_to_timestamp:{:?}", transaction_hash.clone());
		let timestamp = match self.db.get(key)? {
			Some(value) => {
				let bytes: [u8; 16] =
					value.try_into().map_err(|_| Error::msg("Invalid fee used length"))?;
				BlockNumber::from_be_bytes(bytes)
			},
			None => return Err(Error::msg("No timestamp found")),
		};

		let key = format!("tx_hash_to_from_address:{:?}", transaction_hash.clone());
		let from_address: Address = match self.db.get(key)? {
			Some(value) => value.try_into().map_err(|_| Error::msg("Invalid block hash length"))?,
			None => return Err(Error::msg("No block_hash found")),
		};

		let tx_response: TransactionReceiptResponse = TransactionReceiptResponse {
			transaction,
			tx_hash: transaction_hash,
			status: false,
			block_number,
			block_hash,
			fee_used,
			timestamp: timestamp.try_into()?,
			from: from_address,
		};
		Ok(Some(tx_response))
	}

	async fn load_transaction_receipt_by_address(
		&self,
		address: Address,
	) -> Result<Vec<TransactionReceiptResponse>, Error> {
		let tx_hashes = self.get_transactions_for_address(&address)?;
		let mut transactions = Vec::new();
		for tx_hash in tx_hashes {
			let txs: TransactionReceiptResponse =
				match self.load_transaction_receipt(tx_hash, [10u8; 20]).await? {
					Some(tx) => tx,
					None => return Err(Error::msg("No TransactionReceiptResponse found")),
				};
			transactions.push(txs);
		}
		Ok(transactions)
	}

	async fn load_latest_block_headers(
		&self,
		num_blocks: u32,
		cluster_address: Address,
	) -> Result<Vec<l1x_rpc::rpc_model::BlockHeaderV3>, Error> {
		let current_block_number = self.load_chain_state(cluster_address).await?.block_number;
		let lower_block_number = current_block_number - num_blocks as u128;
		let blk_headers = self.get_all_block_headers(lower_block_number)?;
		let mut block_headers: Vec<rpc_model::BlockHeaderV3> = vec![];

		for blk_header in blk_headers {
			let key = format!("block_number_to_timestamp:{:?}", blk_header.block_number);
			let timestamp = match self.db.get(key)? {
				Some(value) => {
					let bytes: [u8; 16] = value
						.try_into()
						.map_err(|_| Error::msg("Invalid timestamp used length"))?;
					BlockNumber::from_be_bytes(bytes)
				},
				None => return Err(Error::msg("No timestamp found")),
			};
			let header = rpc_model::BlockHeaderV3 {
				block_number: blk_header.block_number.try_into().unwrap_or(u64::MIN),
				block_hash: hex::encode(blk_header.block_hash),
				parent_hash: hex::encode(blk_header.parent_hash),
				timestamp: timestamp.try_into().unwrap_or(u64::MIN),
				block_type: blk_header.block_type as i32,
				cluster_address: hex::encode(blk_header.cluster_address),
				num_transactions: blk_header.num_transactions.try_into().unwrap_or(u32::MIN),
				block_version: (blk_header.block_version as u32).to_string(),
				state_hash: hex::encode(blk_header.state_hash),
				epoch: (blk_header.epoch as u64).to_string(),
			};

			block_headers.push(header);
		}
		Ok(block_headers)
	}

	async fn load_latest_transactions(
		&self,
		num_transactions: u32,
	) -> Result<Vec<TransactionResponse>, Error> {
		// Assuming you have a way to iterate over transactions in reverse order
		//let mut iter = self.db.iterator(rocksdb::IteratorMode::End); // Start from the end
		let mut keys = Vec::new();
		let mut iter = self.db.prefix_iterator("block_transaction:");

		for item in iter.by_ref().take(num_transactions as usize) {
			match item {
				Ok((key, _)) => keys.push(key),
				Err(e) => return Err(Error::new(e)),
			}
		}

		// Reverse the vector to simulate starting from the end
		keys.reverse();

		let mut transactions: Vec<TransactionResponse> = Vec::new();
		for key in keys {
			if let Ok(key_str) = String::from_utf8(key.to_vec()) {
				if key_str.starts_with("block_transaction:") {
					// Deserialize the transaction
					let transaction: Transaction = match self.db.get(key_str)? {
						Some(value) => {
							let value_str = String::from_utf8_lossy(&value);
							serde_json::from_str(&value_str)?
						},
						None => return Err(anyhow!("transaction not found")),
					};
					let transaction_hash = transaction.transaction_hash()?;
					let key = format!("tx_hash_to_block_hash:{:?}", transaction_hash.clone());
					let block_hash: BlockHash = match self.db.get(key)? {
						Some(value) =>
							value.try_into().map_err(|_| Error::msg("Invalid block hash length"))?,
						None => return Err(Error::msg("No block_hash found")),
					};
					let key = format!("tx_hash_to_block_number:{:?}", transaction_hash.clone());
					let block_number: BlockNumber = match self.db.get(key)? {
						Some(value) => {
							let bytes: [u8; 16] = value
								.try_into()
								.map_err(|_| Error::msg("Invalid block number length"))?;
							BlockNumber::from_be_bytes(bytes)
						},
						None => return Err(Error::msg("No block_number found")),
					};

					let key = format!("tx_hash_to_fee_used:{:?}", transaction_hash.clone());
					let fee_used = match self.db.get(key)? {
						Some(value) => {
							let bytes: [u8; 16] = value
								.try_into()
								.map_err(|_| Error::msg("Invalid fee used length"))?;
							BlockNumber::from_be_bytes(bytes)
						},
						None => return Err(Error::msg("No fee_used found")),
					};

					let key = format!("tx_hash_to_timestamp:{:?}", transaction_hash.clone());
					let timestamp = match self.db.get(key)? {
						Some(value) => {
							let bytes: [u8; 16] = value
								.try_into()
								.map_err(|_| Error::msg("Invalid fee used length"))?;
							BlockNumber::from_be_bytes(bytes)
						},
						None => return Err(Error::msg("No timestamp found")),
					};

					let key = format!("tx_hash_to_from_address:{:?}", transaction_hash.clone());
					let from_address: Address = match self.db.get(key)? {
						Some(value) =>
							value.try_into().map_err(|_| Error::msg("Invalid block hash length"))?,
						None => return Err(Error::msg("No block_hash found")),
					};
					let rpc_transaction: rpc_model::Transaction =
						to_proto_transaction(transaction.clone())?;
					let response = TransactionResponse {
						transaction_hash: transaction_hash.to_vec(),
						transaction: Some(rpc_transaction),
						from: from_address.to_vec(),
						block_hash: block_hash.to_vec(),
						block_number: block_number.try_into()?,
						fee_used: fee_used.to_string(),
						timestamp: timestamp.try_into()?,
					};
					transactions.push(response);
				} else {
					println!("Invalid key_str");
				}
			}
		}

		Ok(transactions)
	}

	async fn load_transactions(
		&self,
		block_number: BlockNumber,
	) -> Result<Vec<Transaction>, Error> {
		let tx_hashes = self.get_transactions_for_block(block_number)?;
		let mut transactions = Vec::new();
		for tx_hash in tx_hashes {
			let transaction = match self.load_transaction(tx_hash).await? {
				Some(tx) => tx,
				None => return Err(anyhow!("No tx found for hash: {:?}", tx_hash)),
			};
			transactions.push(transaction);
		}
		Ok(transactions)
	}

	async fn load_transactions_response(
		&self,
		block_number: BlockNumber,
	) -> Result<Vec<TransactionReceiptResponse>, Error> {
		let tx_hashes = self.get_transactions_for_block(block_number)?;
		let mut transactions = Vec::new();
		for tx_hash in tx_hashes {
			let transaction = match self.load_transaction(tx_hash).await? {
				Some(tx) => tx,
				None => return Err(anyhow!("No tx found for hash: {:?}", tx_hash)),
			};
			let transaction_hash = transaction.transaction_hash()?;
			let key = format!("tx_hash_to_block_hash:{:?}", transaction_hash.clone());
			let block_hash: BlockHash = match self.db.get(key)? {
				Some(value) =>
					value.try_into().map_err(|_| Error::msg("Invalid block hash length"))?,
				None => return Err(Error::msg("No block_hash found")),
			};
			let key = format!("tx_hash_to_block_number:{:?}", transaction_hash.clone());
			let block_number: BlockNumber = match self.db.get(key)? {
				Some(value) => {
					let bytes: [u8; 16] =
						value.try_into().map_err(|_| Error::msg("Invalid block number length"))?;
					BlockNumber::from_be_bytes(bytes)
				},
				None => return Err(Error::msg("No block_number found")),
			};

			let key = format!("tx_hash_to_fee_used:{:?}", transaction_hash.clone());
			let fee_used = match self.db.get(key)? {
				Some(value) => {
					let bytes: [u8; 16] =
						value.try_into().map_err(|_| Error::msg("Invalid fee used length"))?;
					BlockNumber::from_be_bytes(bytes)
				},
				None => return Err(Error::msg("No fee_used found")),
			};

			let key = format!("tx_hash_to_timestamp:{:?}", transaction_hash.clone());
			let timestamp = match self.db.get(key)? {
				Some(value) => {
					let bytes: [u8; 16] =
						value.try_into().map_err(|_| Error::msg("Invalid fee used length"))?;
					BlockNumber::from_be_bytes(bytes)
				},
				None => return Err(Error::msg("No timestamp found")),
			};

			let key = format!("tx_hash_to_from_address:{:?}", transaction_hash.clone());
			let from_address: Address = match self.db.get(key)? {
				Some(value) =>
					value.try_into().map_err(|_| Error::msg("Invalid block hash length"))?,
				None => return Err(Error::msg("No block_hash found")),
			};
			let txs = TransactionReceiptResponse {
				transaction,
				tx_hash: transaction_hash,
				status: false,
				block_number,
				block_hash,
				fee_used,
				timestamp: timestamp.try_into().unwrap_or(i64::MIN),
				from: from_address,
			};

			transactions.push(txs);
		}
		Ok(transactions)
	}

	async fn is_block_head(&self, cluster_address: &Address) -> Result<bool, Error> {
		let key = format!("block_head:{:?}", cluster_address);
		match self.db.get(key)? {
			Some(_value) => Ok(true), // Block head exists
			None => Ok(false),        // No block head for this address
		}
	}

	async fn is_block_executed(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<bool, Error> {
		let key = format!("block_executed:{:?}:{:?}", cluster_address, block_number);
		match self.db.get(key)? {
			Some(_value) => {
				// Assuming that if the key exists, the block is executed.
				// Adjust logic here if you store more complex data.
				Ok(true)
			},
			None => Ok(false), // The key does not exist, hence the block is not executed
		}
	}

	async fn is_block_header_stored(&self, _block_number: BlockNumber) -> Result<bool, Error> {
		Err(anyhow!("Not supported"))
	}

	async fn set_block_executed(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<(), Error> {
		let key = format!("block_executed:{:?}:{:?}", cluster_address, block_number);
		// Convert the boolean value to a byte slice
		let value = [1u8];
		self.db.put(key, value)?;
		Ok(())
	}
}
