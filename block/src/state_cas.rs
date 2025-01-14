use anyhow::{anyhow, Context, Error};
use async_trait::async_trait;

use db::utils::{FromByteArray, ToByteArray};
use db_traits::{base::BaseState, block::BlockState};
use l1x_rpc::rpc_model::{self, TransactionResponse};
use log::{debug, info};
use primitives::{arithmetic::ScalarBig, *};
use scylla::{_macro_internal::CqlValue, batch::Batch, frame::value::CqlTimestamp, Session};
use std::sync::Arc;
use system::{
	account::Account,
	block::{Block, BlockResponse},
	block_header::BlockHeader,
	chain_state::ChainState,
	transaction::{Transaction, TransactionMetadata},
	transaction_receipt::TransactionReceiptResponse,
};
use util::{
	convert::{bytes_to_address, to_proto_transaction},
	generic::get_cassandra_timestamp,
};
use util::generic::current_timestamp_in_micros;

pub struct StateCas {
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<Account> for StateCas {
	/// Creates necessary tables if they do not exist in the database.
	///
	/// This method creates the following tables:
	/// - `block_meta_info`: Stores metadata information for blocks, including whether they have
	///   been executed.
	/// - `block_header`: Stores header information for blocks, such as block number, hash, parent
	///   hash, timestamp, block type, and number of transactions.
	/// - `block_transaction`: Stores transactions associated with blocks, including transaction
	///   hash, block number, transaction data, block hash, fee used, sender address, timestamp, and
	///   transaction sequence.
	/// - `block_head`: Stores information about the latest block head for each cluster.
	///
	/// # Returns
	///
	/// Returns `Ok(())` if the tables are created successfully, or an `Error` otherwise.
	async fn create_table(&self) -> Result<(), Error> {
		// Create block_meta_info table
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS block_meta_info (
				cluster_address blob,
				block_number BigInt,
				block_executed boolean,
				PRIMARY KEY (cluster_address, block_number)
			);",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Create table block_meta_info failed: {}", e)),
		};

		// Create block_header table
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS block_header (
				block_number BigInt,
				block_hash blob,
				parent_hash blob,
				timestamp Timestamp,
				block_type Tinyint,
				num_transactions Int,
				PRIMARY KEY (block_number)
			);",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Create table block_header failed: {}", e)),
		};

		// Create block_transaction table
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS block_transaction (
				transaction_hash blob,
				block_number BigInt,
				transaction blob,
				block_hash blob,
				fee_used blob,
				from_address blob,
				timestamp Timestamp,
				tx_sequence BigInt,
				PRIMARY KEY (transaction_hash, block_number)
			)   WITH CLUSTERING ORDER BY (block_number ASC);",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Create table block_transaction failed: {}", e)),
		};

		// Create block_head table
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS block_head (
				cluster_address blob,
				block_number BigInt,
				block_hash blob,
				PRIMARY KEY (cluster_address)
			);",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Create table block_head failed: {}", e)),
		};
		Ok(())
	}

	async fn create(&self, _u: &Account) -> Result<(), Error> {
		todo!()
	}

	async fn update(&self, _u: &Account) -> Result<(), Error> {
		todo!()
	}

	/// Executes a raw database query.
	///
	/// # Arguments
	///
	/// * `query`: A string containing the raw query to be executed.
	///
	/// # Returns
	///
	/// Returns `Ok(())` if the query is executed successfully, or an `Error` otherwise.
	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match self.session.query(query, &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed to execute raw query: {:?}", e)),
		};
		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		todo!()
	}
}

#[async_trait]
impl BlockState for StateCas {
	/// Stores the block head information in the database.
	///
	/// # Arguments
	///
	/// * `cluster_address`: The address of the cluster.
	/// * `block_number`: The number of the block.
	/// * `block_hash`: The hash of the block.
	///
	/// # Returns
	///
	/// Returns `Ok(())` if the operation is successful, or an `Error` otherwise.
	async fn store_block_head(
		&self,
		cluster_address: &Address,
		block_number: BlockNumber,
		block_hash: BlockHash,
	) -> Result<(), Error> {
		// Convert the block number to an i64
		let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);

		// Execute the INSERT query
		match self
			.session
			.query(
				"INSERT INTO block_head (cluster_address, block_number, block_hash) VALUES (?, ?, ?);",
				(cluster_address, &block_number, block_hash),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Store last block number failed - {}", e)),
		}
	}

	/// Updates the block head information in the database.
	///
	/// If the block head for the specified cluster address does not exist, it will be created.
	///
	/// # Arguments
	///
	/// * `cluster_address`: The address of the cluster.
	/// * `big_block_number`: The number of the block as an i64.
	/// * `block_hash`: The hash of the block.
	///
	/// # Returns
	///
	/// Returns `Ok(())` if the operation is successful, or an `Error` otherwise.
	async fn update_block_head(
		&self,
		cluster_address: Address,
		big_block_number: BlockNumber,
		block_hash: BlockHash,
	) -> Result<(), Error> {
		// Convert the block number to an i64
		let block_number: i64 = i64::try_from(big_block_number).unwrap_or(i64::MAX);

		// Check if the block head for the cluster address exists
		if !self.is_block_head(&cluster_address).await? {
			// If it doesn't exist, store the block head
			self.store_block_head(&cluster_address, big_block_number, block_hash).await
		} else {
			// If it exists, update the block head
			match self
				.session
				.query(
					"UPDATE block_head set block_number = ?, block_hash = ? WHERE cluster_address = ?;",
					(&block_number, &block_hash, &cluster_address),
				)
				.await
			{
				Ok(_) => Ok(()),
				Err(_) => {
					// If the update fails, store the block head
					self.store_block_head(&cluster_address, big_block_number, block_hash)
						.await
				}
			}
		}
	}

	async fn load_genesis_block_time(
		&self,
	) -> Result<(u64, u64), Error> {
		todo!()
	}

	async fn store_genesis_block_time(
		&self,
		block_number: BlockNumber,
		epoch: Epoch,
	) -> Result<(), Error> {
		// Convert the block number and epoch to an i64
		let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
		let gen_epoch: i64 = i64::try_from(epoch).unwrap_or(i64::MAX);
		// Execute the INSERT query
		match self
			.session
			.query(
				"INSERT INTO genesis_epoch_block (block_number, epoch) VALUES (?, ?);",
				(&block_number, gen_epoch),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Store genesis block number failed - {}", e)),
		}
	}

	async fn load_chain_state(&self, cluster_address: Address) -> Result<ChainState, Error> {
		let query_result = match self
			.session
			.query("SELECT * FROM block_head WHERE cluster_address = ?;", (&cluster_address,))
			.await
		{
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Invalid cluster id - {}", e)),
		};

		if let Some(mut rows) = query_result.rows {
			match rows.pop() {
				Some(row) => {
					// debug!("block_header row: {:?}", row);
					// info!("block_head row {:?}", row);
					let (address, hash, number) = row.into_typed::<(Address, [u8; 32], i64)>()?;

					Ok(ChainState {
						cluster_address: address,
						block_number: number.try_into().unwrap(),
						block_hash: hash,
					})
				},
				None => {
					info!("None ***********************");
					//return Err(anyhow!("No row for block header  found"))
					Ok(ChainState { cluster_address, block_number: 0, block_hash: [0u8; 32] })
				},
			}
		} else {
			info!("ERROR ***********************");
			//Err(anyhow!("No vector of rows for block header found"))
			Ok(ChainState { cluster_address, block_number: 0, block_hash: [0u8; 32] })
		}

		// if rows.is_empty() {
		//     return Err(anyhow!("No rows found"));
		// }
		// let row = rows.get(0).ok_or_else(|| anyhow!("No row found"))?;
		// // let (block_head, cluster_address, block_number, block_hash) =
		// //     row.clone().into_typed::<([u8; 32], Address, i64, [u8; 32])>()?;
		// let (block_head, cluster_address, block_number, block_hash) = row
		//     .into_typed::<([u8; 32], Address, i64, [u8; 32])>()?
		//     .clone();

		// let row = rows.get(0).ok_or_else(|| anyhow!("No row found"))?;
		// for row in rows {
		//     let (block_head, cluster_address, block_number, block_hash) =
		//         row.into_typed::<([u8; 32], Address, i64, [u8; 32])>()?;
		//     break;
		// }

		// let mut block_head: Option<[u8; 32]> = None;
		// let mut cluster_address: Option<Address> = None;
		// let mut block_number: Option<i64> = None;
		// let mut block_hash: Option<[u8; 32]> = None;

		// for row in rows {
		//     // Update the variables
		//     block_head = Some(head);
		//     cluster_address = Some(address);
		//     block_number = Some(number);
		//     block_hash = Some(hash);
		//     print!(
		//         "{:?} {:?} {:?} {:?}",
		//         block_head, cluster_address, block_number, block_hash
		//     );
		//     // Stop after the first iteration
		//     break;
		// }

		// // Use the variables outside the loop, note that they are Option types
		// if let (Some(head), Some(address), Some(number), Some(hash)) =
		//     (block_head, cluster_address, block_number, block_hash)
		// {
		//     // Now the variables head, address, number, and hash can be used here
		//     let cluster_address = match cluster_address {
		//         Some(cluster) => cluster,
		//         None => return Err(anyhow!("No rows found")),
		//     };

		//     let block_number = match block_number {
		//         Some(number) => number,
		//         None => return Err(anyhow!("No rows found")),
		//     };

		//     let block_hash = match block_hash {
		//         Some(hash) => hash,
		//         None => return Err(anyhow!("No rows found")),
		//     };
		//     Ok(ChainState {
		//         cluster_address: cluster_address,
		//         block_number: block_number.try_into().unwrap(),
		//         block_hash: block_hash,
		//     })
		// } else {
		//     // Handle the case where the values were not set
		//     return Err(anyhow!("No rows found"));
		// }

		// let (block_number_idx, _) = query_result
		//     .get_column_spec("block_number")
		//     .ok_or_else(|| anyhow!("No block_number column found"))?;
		// let (block_hash_idx, _) = query_result
		//     .get_column_spec("block_hash")
		//     .ok_or_else(|| anyhow!("No block_hash column found"))?;

		// let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		// let block_number: BlockNumber = if let Some(row) = rows.get(0) {
		//     if let Some(block_number_value) = &row.columns[block_number_idx] {
		//         if let CqlValue::Blob(block_number) = block_number_value {
		//             u128::from_byte_array(block_number)
		//         } else {
		//             return Err(anyhow!("Unable to convert to BlockNumber type"));
		//         }
		//     } else {
		//         return Err(anyhow!("Unable to read block_number column"));
		//     }
		// } else {
		//     return Err(anyhow!(
		//         "block_state : block_number 187: Unable to read row"
		//     ));
		// };
		// let block_hash: BlockHash = if let Some(row) = rows.get(0) {
		//     if let Some(block_hash_value) = &row.columns[block_hash_idx] {
		//         if let CqlValue::Blob(block_hash) = block_hash_value {
		//             block_hash
		//                 .as_slice()
		//                 .try_into()
		//                 .map_err(|_| anyhow!("Invalid length"))?
		//         } else {
		//             return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"));
		//         }
		//     } else {
		//         return Err(anyhow!("Unable to read block_hash column"));
		//     }
		// } else {
		//     return Err(anyhow!(
		//         "block_state : block_header_data 203: Unable to read row"
		//     ));
		// };
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
		let block_number = i64::try_from(block.block_header.block_number).unwrap_or(i64::MAX);
		// let num_transactions = u32::try_from((&block.transactions).len()).unwrap_or(u32::MAX);

		info!(
			"ℹ️  Storing block #{} for cluster 0x{}",
			block_number,
			hex::encode(block.block_header.cluster_address)
		);

		match self
			.session
			.query(
				"INSERT INTO block_meta_info (cluster_address, block_number, block_executed) VALUES (?, ?, ?);",
				(&block.block_header.cluster_address, &block_number, false),
			)
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Store block failed - {}", e)),
		};

		self.store_block_header(block.block_header.clone()).await?;
		let transactions = block.transactions.clone();
		for (sequence, transaction) in transactions.iter().enumerate() {
			self.store_transaction(
				block.block_header.block_number,
				block.block_header.block_hash,
				transaction.clone(),
				sequence as TransactionSequence,
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

	/// Store a batch of blocks. This should be more efficient than storing them one by one.
	/// Assumes that the blocks are already sorted by block number.
	async fn batch_store_blocks(&self, blocks: Vec<Block>) -> Result<(), Error> {
		let last_block = match blocks.last() {
			Some(block) => block.clone(),
			None => return Err(anyhow!("No blocks to store")),
		};

		// Create a batch statement
		let mut blocks_batch: Batch = Default::default();
		// Holds the input values for the batch queries
		let mut blocks_batch_values = Vec::new();

		let mut block_headers: Vec<BlockHeader> = Vec::new();

		let mut transactions: Vec<(BlockNumber, BlockHash, Transaction)> = Vec::new();

		for block in blocks {
			let block_number = i64::try_from(block.block_header.block_number).unwrap_or(i64::MAX);

			info!(
				"ℹ️  Storing block #{} for cluster 0x{}",
				block_number,
				hex::encode(block.block_header.cluster_address),
			);

			// block_meta_info
			blocks_batch.append_statement("INSERT INTO block_meta_info (cluster_address, block_number, block_executed) VALUES (?, ?, ?);");
			blocks_batch_values.push((block.block_header.cluster_address, block_number, false));

			// block_header
			block_headers.push(block.block_header.clone());

			let block_hash = block.block_header.block_hash;

			for transaction in block.transactions {
				transactions.push((block.block_header.block_number, block_hash, transaction));
			}
		}

		// Prepare all statements in the batch at once
		let prepared_batch: Batch = self.session.prepare_batch(&blocks_batch).await?;
		// Run the batch
		self.session.batch(&prepared_batch, blocks_batch_values).await?;

		// Store block headers
		self.batch_store_block_headers(block_headers).await?;

		// Store transactions
		self.batch_store_transactions(transactions).await?;

		// Update block_head
		self.update_block_head(
			last_block.block_header.cluster_address,
			last_block.block_header.block_number,
			last_block.block_header.block_hash,
		)
		.await?;

		Ok(())
	}

	/// Store a single transaction
	async fn store_transaction(
		&self,
		block_number: BlockNumber,
		block_hash: BlockHash,
		transaction: Transaction,
		tx_sequence: TransactionSequence,
	) -> Result<(), Error> {
		let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
		let fee_used_bytes: ScalarBig =
			transaction.fee_limit.to_byte_array_le(ScalarBig::default());
		let transaction_bytes: Vec<u8> = bincode::serialize(&transaction)?;
		let transaction_hash = transaction.transaction_hash()?;
		let from_address = Account::address(&transaction.verifying_key)?;
		let timestamp = get_cassandra_timestamp()?;
		let tx_sequence: i64 = i64::from(tx_sequence);

		match self
			.session
			.query(
				"INSERT INTO block_transaction (transaction_hash, block_number, transaction, block_hash, fee_used, from_address, timestamp, tx_sequence) VALUES (?, ?, ?, ?, ?, ?, ?, ?);",
				(&transaction_hash, &block_number, &transaction_bytes, block_hash, fee_used_bytes, from_address, timestamp, tx_sequence),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Store transaction failed - {}", e)),
		}
	}

	/// Store a batch of transactions. This should be more efficient than storing them one by one.
	async fn batch_store_transactions(
		&self,
		transactions: Vec<(BlockNumber, BlockHash, Transaction)>,
	) -> Result<(), Error> {
		// Create a batch statement
		let mut batch: Batch = Default::default();

		let mut batch_values = Vec::new();

		for (tx_sequence, (block_number, block_hash, transaction)) in
			transactions.iter().enumerate()
		{
			let block_number: i64 = i64::try_from(*block_number).unwrap_or(i64::MAX);
			let fee_used_bytes: ScalarBig =
				transaction.clone().fee_limit.to_byte_array_le(ScalarBig::default());
			let transaction_bytes: Vec<u8> = bincode::serialize(&transaction)?;
			let transaction_hash = transaction.clone().transaction_hash()?;
			let from_address = Account::address(&transaction.verifying_key)?;
			let timestamp = get_cassandra_timestamp()?;
			let tx_sequence: i64 =
				i64::try_from(tx_sequence as TransactionSequence).unwrap_or(i64::MAX);

			batch_values.push((
				transaction_hash,
				block_number,
				transaction_bytes,
				block_hash,
				fee_used_bytes,
				from_address,
				timestamp,
				tx_sequence,
			));

			batch.append_statement("INSERT INTO block_transaction (transaction_hash, block_number, transaction, block_hash, fee_used, from_address, timestamp, tx_sequence) VALUES (?, ?, ?, ?, ?, ?, ?, ?);");
		}

		// Prepare all statements in the batch at once
		let prepared_batch: Batch = self.session.prepare_batch(&batch).await?;

		// Run the batch
		self.session.batch(&prepared_batch, batch_values).await?;

		Ok(())
	}

	async fn store_transaction_metadata(&self, metadata: TransactionMetadata) -> Result<(), Error> {
		let fee_used_bytes: ScalarBig =
			metadata.fee.to_byte_array_le(ScalarBig::default());
		let gas_burnt_bytes: ScalarBig =
			metadata.burnt_gas.to_byte_array_le(ScalarBig::default());
		match self
			.session
			.query(
				"INSERT INTO transaction_metadata (transaction_hash, fee, burnt_gas, is_successful) VALUES (?, ?, ?, ?);",
				(&metadata.transaction_hash, fee_used_bytes, gas_burnt_bytes, metadata.is_successful),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Store transaction failed - {}", e)),
		}
	}

	async fn load_transaction_metadata(
		&self,
		tx_hash: TransactionHash,
	) -> Result<TransactionMetadata, Error> {
		Err(anyhow!("Cassandra transaction_metadata is not implemented"))
	}

	// FIXME: mitigate "ALLOW FILTERING" with partition keys/secondary indexes
	async fn get_transaction_count(
		&self,
		address: &Address,
		block_number: Option<BlockNumber>,
	) -> Result<u64, Error> {
		let (tx_count,) = match block_number {
			Some(block_number) => {
				// let block_number: ScalarBig = block_number.to_byte_array_le(ScalarBig::default());
				let block_number = i64::try_from(block_number).unwrap_or(i64::MAX);
				self.session.query("SELECT COUNT(*) FROM block_transaction WHERE from_address = ? AND block_number <= ? ALLOW FILTERING;", (address, &block_number)).await?.single_row()?.into_typed::<(i64,)>()?
			}
			None => self
				.session
				.query(
					"SELECT COUNT(*) FROM block_transaction WHERE from_address = ? ALLOW FILTERING;",
					(address,),
				)
				.await?
				.single_row()?
				.into_typed::<(i64,)>()?,
		};
		Ok(tx_count as u64)
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
		let block_number = i64::try_from(block_header.block_number).unwrap_or(i64::MAX);
		let block_hash = block_header.block_hash;
		let parent_hash = block_header.block_hash;
		let milliseconds = i64::try_from(block_header.timestamp).unwrap_or(i64::MAX);
		//let timestamp = Duration::microseconds(milliseconds);
		let block_type: i8 = block_header.block_type.into();
		match self.session.query(
			"INSERT INTO block_header (block_number, block_hash, parent_hash, timestamp, block_type, num_transactions) VALUES (?, ?, ?, ?, ?, ?);",
			(&block_number, &block_hash, &parent_hash, &CqlTimestamp(milliseconds), &block_type, block_header.num_transactions),
		)
			.await {
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Storing block header failed: {:?}", e)),
		}
	}

	/// Stores a batch of BlockHeaders. This should be more efficient than storing them one by one.
	async fn batch_store_block_headers(
		&self,
		block_headers: Vec<BlockHeader>,
	) -> Result<(), Error> {
		// Create a batch statement
		let mut batch: Batch = Default::default();

		let mut batch_values = Vec::new();

		for block_header in block_headers {
			let block_number = i64::try_from(block_header.block_number).unwrap_or(i64::MAX);
			let block_hash = block_header.block_hash;
			let parent_hash = block_header.parent_hash;
			let timestamp = current_timestamp_in_micros()?; // Convert the duration to microseconds
			let milliseconds = i64::try_from(timestamp).unwrap_or(i64::MAX);
			//let timestamp = Duration::microseconds(milliseconds);
			let block_type: i8 = block_header.block_type.into();
			let num_transactions = block_header.num_transactions;

			batch_values.push((
				block_number,
				block_hash,
				parent_hash,
				CqlTimestamp(milliseconds),
				block_type,
				num_transactions,
			));

			batch.append_statement("INSERT INTO block_header (block_number, block_hash, parent_hash, timestamp, block_type, num_transactions) VALUES (?, ?, ?, ?, ?, ?);");
		}

		// Prepare all statements in the batch at once
		let prepared_batch: Batch = self.session.prepare_batch(&batch).await?;

		// Run the batch
		self.session.batch(&prepared_batch, batch_values).await?;

		Ok(())
	}

	async fn load_block_header(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<BlockHeader, Error> {
		// let block_number_bytes: ScalarBig = block_number.to_byte_array_le(ScalarBig::default());
		let block_number = i64::try_from(block_number).unwrap_or(i64::MAX);

		let query_result = match self
			.session
			.query("SELECT * FROM block_header WHERE block_number = ?;", (&block_number,))
			.await
		{
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Failed loading block #{} header: {}", block_number, e)),
		};

		if let Some(mut rows) = query_result.rows {
			if rows.len() > 1 {
				return Err(anyhow!(
					"Should never happen. More than one row for block header #{} found",
					block_number
				));
			}

			match rows.pop() {
				Some(row) => {
					debug!("block_header row: {:?}", row);
					let (
						block_number,
						block_hash,
						block_type,
						num_transactions,
						parent_hash,
						timestamp,
						state_hash,
						block_version,
						epoch
					) = row.into_typed::<(i64, [u8; 32], i8, i32, [u8; 32], CqlTimestamp, [u8; 32], i32, i64)>()?;

					let timestamp: BlockTimeStamp = timestamp.0.try_into()?;

					let header = BlockHeader {
						block_number: block_number.try_into().unwrap_or(BlockNumber::MIN),
						block_hash,
						parent_hash,
						block_type: block_type.try_into()?,
						timestamp: timestamp / 1000,
						cluster_address: *cluster_address,
						num_transactions: num_transactions.try_into().unwrap_or(i32::MIN),
						block_version: block_version.try_into().unwrap_or(u32::MIN),
						state_hash,
						epoch: epoch as u64,
					};
					Ok(header)
				},
				None => return Err(anyhow!("No row for block header #{} found", block_number)),
			}
		} else {
			Err(anyhow!("No vector of rows for block header #{} found", block_number))
		}
	}

	async fn load_transaction(
		&self,
		transaction_hash: TransactionHash,
	) -> Result<Option<Transaction>, Error> {
		let query_result = match self
			.session
			.query(
				"SELECT * FROM block_transaction WHERE transaction_hash = ?;",
				(&transaction_hash,),
			)
			.await
		{
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid transaction_hash")),
		};

		let (transaction_idx, _) = query_result
			.get_column_spec("transaction")
			.ok_or_else(|| anyhow!("No transaction column found"))?;

		let rows = query_result.rows.context("SELECT should return vec of rows")?;

		if rows.is_empty() {
			return Ok(None);
		} else {
			let transaction: Transaction = if let Some(row) = rows.first() {
				if let Some(transaction_value) = &row.columns[transaction_idx] {
					if let CqlValue::Blob(transaction) = transaction_value {
						bincode::deserialize(transaction)
							.map_err(|_| anyhow!("Unable to deserialize to Transaction type"))?
					} else {
						return Err(anyhow!("Unable to convert to Transaction type"));
					}
				} else {
					return Err(anyhow!("Unable to read transaction column"));
				}
			} else {
				return Err(anyhow!("block_state : block_header_data 484: Unable to read row"));
			};
			Ok(Some(transaction))
		}
	}

	async fn load_transaction_receipt(
		&self,
		transaction_hash: TransactionHash,
		_cluster_address: Address
	) -> Result<Option<TransactionReceiptResponse>, Error> {
		let query_result = match self
			.session
			.query(
				"SELECT * FROM block_transaction WHERE transaction_hash = ?;",
				(&transaction_hash,),
			)
			.await
		{
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid transaction_hash")),
		};
		if let Some(mut rows) = query_result.rows {
			if rows.len() > 1 {
				return Err(anyhow!(
					"Should never happen. More than one row for tx_hash {:?} found",
					transaction_hash
				));
			}
			match rows.pop() {
				Some(row) => {
					let (
						tx_hash,
						block_number,
						block_hash,
						fee_used,
						from_address,
						timestamp,
						transaction,
						_tx_sequence,
					) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, CqlTimestamp, Vec<u8>, i64)>()?;
					let sys_transaction: Transaction = bincode::deserialize(&transaction)?;
					// let rpc_transaction: rpc_model::Transaction =
					// to_proto_transaction(sys_transaction).await?;
					let fee_used = u128::from_byte_array(&fee_used);
					// let timestamp = u64::try_from(timestamp).unwrap_or(u64::MIN);
					let mut block_hash_arr: [u8; 32] = [0u8; 32];
					block_hash_arr.copy_from_slice(&block_hash);

					let mut tx_hash_arr: [u8; 32] = [0u8; 32];
					tx_hash_arr.copy_from_slice(&tx_hash);

					// let tx_response = TransactionResponse {
					//     transaction_hash: tx_hash,
					//     transaction: Some(rpc_transaction),
					//     from: from_address,
					//     block_hash: block_hash.to_vec(),
					//     block_number,
					//     fee_used,
					//     timestamp,
					// };

					let tx_response: TransactionReceiptResponse = TransactionReceiptResponse {
						transaction: sys_transaction,
						tx_hash: tx_hash_arr,
						status: false,
						block_number: block_number.try_into().unwrap_or(BlockNumber::MIN),
						block_hash: block_hash_arr,
						fee_used,
						timestamp: timestamp.0,
						from: bytes_to_address(from_address)?,
					};
					Ok(Some(tx_response))
				},
				None =>
					return Err(anyhow!("No row for transaction hash {:?} found", transaction_hash)),
			}
		} else {
			Err(anyhow!("No vector of rows for transaction hash {:?} found", transaction_hash))
		}

		// let (transaction_idx, _) = query_result
		//     .get_column_spec("transaction")
		//     .ok_or_else(|| anyhow!("No transaction column found"))?;
		// let (block_number_idx, _) = query_result
		//     .get_column_spec("block_number")
		//     .ok_or_else(|| anyhow!("No block_number column found"))?;
		// let (block_hash_idx, _) = query_result
		//     .get_column_spec("block_hash")
		//     .ok_or_else(|| anyhow!("No block_hash column found"))?;
		// let (fee_used_idx, _) = query_result
		//     .get_column_spec("fee_used")
		//     .ok_or_else(|| anyhow!("No timestamp column found"))?;

		// let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		// let transaction: Transaction = if let Some(row) = rows.get(0) {
		//     if let Some(transaction_value) = &row.columns[transaction_idx] {
		//         if let CqlValue::Blob(transaction) = transaction_value {
		//             bincode::deserialize(transaction)
		//                 .map_err(|_| anyhow!("Unable to deserialize to Transaction type"))?
		//         } else {
		//             return Err(anyhow!("Unable to convert to Transaction type"));
		//         }
		//     } else {
		//         return Err(anyhow!("Unable to read transaction column"));
		//     }
		// } else {
		//     return Err(anyhow!(
		//         "block_state : block_header_data 532: Unable to read row"
		//     ));
		// };

		// let block_number: BlockNumber = if let Some(row) = rows.get(0) {
		//     if let Some(block_number_value) = &row.columns[block_number_idx] {
		//         if let CqlValue::Blob(block_number) = block_number_value {
		//             u128::from_byte_array(block_number)
		//         } else {
		//             return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"));
		//         }
		//     } else {
		//         return Err(anyhow!("Unable to read block_header_data column"));
		//     }
		// } else {
		//     return Err(anyhow!(
		//         "block_state : block_header_data 546: Unable to read row"
		//     ));
		// };

		// let block_hash: BlockHash = if let Some(row) = rows.get(0) {
		//     if let Some(block_hash_value) = &row.columns[block_hash_idx] {
		//         if let CqlValue::Blob(block_hash) = block_hash_value {
		//             block_hash
		//                 .as_slice()
		//                 .try_into()
		//                 .map_err(|_| anyhow!("Invalid length"))?
		//         } else {
		//             return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"));
		//         }
		//     } else {
		//         return Err(anyhow!("Unable to read block_hash column"));
		//     }
		// } else {
		//     return Err(anyhow!(
		//         "block_state : block_header_data 563: Unable to read row"
		//     ));
		// };

		// let fee_used: Balance = if let Some(row) = rows.get(0) {
		//     if let Some(fee_used_value) = &row.columns[fee_used_idx] {
		//         if let CqlValue::Blob(fee_used) = fee_used_value {
		//             u128::from_byte_array(fee_used)
		//         } else {
		//             return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"));
		//         }
		//     } else {
		//         return Err(anyhow!("Unable to read fee_used column"));
		//     }
		// } else {
		//     return Err(anyhow!(
		//         "block_state : block_header_data 577: Unable to read row"
		//     ));
		// };
	}

	async fn load_transaction_receipt_by_address(
		&self,
		address: Address,
	) -> Result<Vec<TransactionReceiptResponse>, Error> {
		let query_result = match self
			.session
			.query(
				"SELECT * FROM block_transaction WHERE from_address = ? ALLOW FILTERING;",
				(&address,),
			)
			.await
		{
			Ok(q) => q,
			Err(e) =>
				return Err(anyhow!("load_transaction_receipt_by_address: Invalid address - {}", e)),
		};

		let mut transactions: Vec<TransactionReceiptResponse> = vec![];
		if let Some(rows) = query_result.rows {
			for row in rows {
				let (
					tx_hash,
					block_number,
					block_hash,
					fee_used,
					from_address,
					timestamp,
					transaction,
					_tx_sequence,
				) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, CqlTimestamp, Vec<u8>, i64)>()?;

				let sys_transaction: Transaction = bincode::deserialize(&transaction)?;
				// let rpc_transaction: rpc_model::Transaction =
				// to_proto_transaction(sys_transaction).await?;
				let fee_used = u128::from_byte_array(&fee_used);
				// let timestamp = u64::try_from(timestamp).unwrap_or(u64::MIN);
				let mut block_hash_arr: [u8; 32] = [0u8; 32];
				block_hash_arr.copy_from_slice(&block_hash);

				let mut tx_hash_arr: [u8; 32] = [0u8; 32];
				tx_hash_arr.copy_from_slice(&tx_hash);

				let txs: TransactionReceiptResponse = TransactionReceiptResponse {
					transaction: sys_transaction,
					tx_hash: tx_hash_arr,
					status: false,
					block_number: block_number.try_into().unwrap_or(BlockNumber::MIN),
					block_hash: block_hash_arr,
					fee_used,
					timestamp: timestamp.0,
					from: bytes_to_address(from_address)?,
				};

				transactions.push(txs);
			}
		} else {
			return Err(anyhow!("No vector of rows for address 0x{:?}", address));
		}

		Ok(transactions)
	}

	/// Load the latest x number of block headers
	async fn load_latest_block_headers(
		&self,
		num_blocks: u32,
		cluster_address: Address,
	) -> Result<Vec<rpc_model::BlockHeaderV3>, Error> {
		let current_block_number = self.load_chain_state(cluster_address).await?.block_number;
		let lower_block_number = current_block_number - num_blocks as u128;
		let lower_block_number_i64 = i64::try_from(lower_block_number).unwrap_or(i64::MAX);

		let query = r#"
			SELECT * FROM block_header
			WHERE block_number > ?
			ALLOW FILTERING;
		"#;

		let query_result = match self.session.query(query, (&lower_block_number_i64,)).await {
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Failed loading latest block headers: {}", e)),
		};

		let mut block_headers: Vec<rpc_model::BlockHeaderV3> = vec![];
		if let Some(rows) = query_result.rows {
			for row in rows {
				debug!("row: {:?}", row);

				let (
					block_number,
					block_hash,
					block_type,
					num_transactions,
					parent_hash,
					timestamp,
					state_hash,
					block_version,
					epoch,
				) = row.into_typed::<(i64, [u8; 32], i8, i32, [u8; 32], CqlTimestamp, [u8; 32], i32, i32)>()?;

				let header = rpc_model::BlockHeaderV3 {
					block_number: block_number.try_into().unwrap_or(u64::MIN),
					block_hash: hex::encode(block_hash),
					parent_hash: hex::encode(parent_hash),
					timestamp: timestamp.0.try_into().unwrap_or(u64::MIN),
					block_type: block_type.into(),
					cluster_address: hex::encode([0u8; 20]),
					num_transactions: num_transactions.try_into().unwrap_or(u32::MIN),
					block_version: block_version.to_string(),
					state_hash:  hex::encode(state_hash),
					epoch: epoch.try_into().unwrap_or(u64::MIN).to_string(),
				};

				block_headers.push(header);
			}
		}
		Ok(block_headers)
	}

	/// Load the latest x number of transactions
	async fn load_latest_transactions(
		&self,
		num_transactions: u32,
	) -> Result<Vec<TransactionResponse>, Error> {
		let num_transactions = i32::try_from(num_transactions).unwrap_or(i32::MAX);

		let query = r#"
			SELECT * FROM block_transaction
			LIMIT ?;
		"#;

		let query_result = match self.session.query(query, (num_transactions,)).await {
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Failed loading latest transactions: {}", e)),
		};

		let mut transactions: Vec<TransactionResponse> = vec![];
		if let Some(rows) = query_result.rows {
			for row in rows {
				let (
					tx_hash,
					block_number,
					block_hash,
					fee_used,
					from_address,
					timestamp,
					transaction,
					_tx_sequence,
				) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, CqlTimestamp, Vec<u8>, i64)>()?;

				let sys_transaction: Transaction = bincode::deserialize(&transaction)?;
				let rpc_transaction: rpc_model::Transaction =
					to_proto_transaction(sys_transaction)?;
				let fee_used = u128::from_byte_array(&fee_used).to_string();
				let timestamp = u64::try_from(timestamp.0).unwrap_or(u64::MIN);

				let txs = TransactionResponse {
					transaction_hash: tx_hash,
					transaction: Some(rpc_transaction),
					from: from_address,
					block_hash: block_hash.to_vec(),
					block_number,
					fee_used,
					timestamp,
				};

				transactions.push(txs);
			}
		}
		Ok(transactions)
	}

	/// Load the transactions for a given block number
	async fn load_transactions(
		&self,
		block_number: BlockNumber,
	) -> Result<Vec<Transaction>, Error> {
		// let block_number_bytes: ScalarBig = block_number.to_byte_array_le(ScalarBig::default());
		let block_number = i64::try_from(block_number).unwrap_or(i64::MAX);

		let query_result = match self
			.session
			.query(
				"SELECT * FROM block_transaction WHERE block_number = ? ALLOW FILTERING;",
				(&block_number,),
			)
			.await
		{
			Ok(q) => q,
			Err(e) =>
				return Err(anyhow!("Failed loading block #{} transactions - {}", block_number, e)),
		};

		let mut transactions_seq: Vec<(i64, Transaction)> = vec![];
		if let Some(rows) = query_result.rows {
			for row in rows {
				let (
					_tx_hash,
					_block_number,
					_block_hash,
					_fee_used,
					_from_address,
					_timestamp,
					transaction,
					tx_sequence,
				) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, CqlTimestamp, Vec<u8>, i64)>()?;

				let sys_transaction: Transaction = bincode::deserialize(&transaction)?;
				transactions_seq.push((tx_sequence, sys_transaction));
			}
		}

		transactions_seq.sort_by_key(|(sequence, _)| *sequence);
		let transactions: Vec<Transaction> =
			transactions_seq.into_iter().map(|(_, transaction)| transaction).collect();
		Ok(transactions)
	}

	/// Load the transactions for a given block number
	async fn load_transactions_response(
		&self,
		block_number: BlockNumber,
	) -> Result<Vec<TransactionReceiptResponse>, Error> {
		// let block_number_bytes: ScalarBig = block_number.to_byte_array_le(ScalarBig::default());
		let block_number = i64::try_from(block_number).unwrap_or(i64::MAX);

		let query_result = match self
			.session
			.query(
				"SELECT * FROM block_transaction WHERE block_number = ? ALLOW FILTERING;",
				(&block_number,),
			)
			.await
		{
			Ok(q) => q,
			Err(e) =>
				return Err(anyhow!("Failed loading block #{} transactions - {}", block_number, e)),
		};

		let mut transactions: Vec<TransactionReceiptResponse> = vec![];
		if let Some(rows) = query_result.rows {
			for row in rows {
				let (
					tx_hash,
					block_number,
					block_hash,
					fee_used,
					from_address,
					timestamp,
					transaction,
					_tx_sequence,
				) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, CqlTimestamp, Vec<u8>, i64)>()?;

				let sys_transaction: Transaction = bincode::deserialize(&transaction)?;
				// let rpc_transaction: rpc_model::Transaction =
				// to_proto_transaction(sys_transaction).await?;
				let fee_used = u128::from_byte_array(&fee_used);
				// let timestamp = u64::try_from(timestamp).unwrap_or(u64::MIN);

				let mut tx_hash_arr = [0u8; 32];
				tx_hash_arr.copy_from_slice(&tx_hash);

				let mut block_hash_arr = [0u8; 32];
				block_hash_arr.copy_from_slice(&block_hash);

				// let txs = TransactionResponse {
				//     transaction_hash: tx_hash,
				//     transaction: Some(rpc_transaction),
				//     from: from_address,
				//     block_hash: block_hash.to_vec(),
				//     block_number,
				//     fee_used,
				//     timestamp,
				// };

				let txs = TransactionReceiptResponse {
					transaction: sys_transaction,
					tx_hash: tx_hash_arr,
					status: false,
					block_number: block_number.try_into().unwrap_or(BlockNumber::MIN),
					block_hash: block_hash_arr,
					fee_used,
					timestamp: timestamp.0,
					from: bytes_to_address(from_address)?,
				};

				transactions.push(txs);
			}
		}

		Ok(transactions)
	}

	async fn is_block_head(&self, cluster_address: &Address) -> Result<bool, Error> {
		let query_result = match self
			.session
			.query(
				"SELECT COUNT(*) AS count FROM block_head WHERE cluster_address = ?;",
				(&cluster_address,),
			)
			.await
		{
			Ok(q) => q,
			Err(_) => return Err(anyhow!("Invalid address")),
		};

		let (count_idx, _) = query_result
			.get_column_spec("count")
			.ok_or_else(|| anyhow!("No count column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		if let Some(row) = rows.first() {
			if let Some(count_value) = &row.columns[count_idx] {
				if let CqlValue::BigInt(count) = count_value {
					return Ok(count > &0);
				} else {
					return Err(anyhow!("Unable to convert to Nonce type"));
				}
			}
		}
		Ok(false)
	}

	async fn is_block_executed(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<bool, Error> {
		let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
		let (block_executed,) = self
			.session
			.query("SELECT block_executed FROM block_meta_info WHERE block_number = ? AND cluster_address = ? allow filtering;", (block_number, cluster_address))
			.await?
			.single_row().map_err(|e| anyhow!("Failed to get only a single row when querying the block_meta_info table for block #{} and cluster address {:?}: {}", block_number, cluster_address, e))?
			.into_typed::<(bool,)>()?;

		Ok(block_executed)
	}

	async fn is_block_header_stored(&self, _block_number: BlockNumber) -> Result<bool, Error> {
		Err(anyhow!("Not supported"))
	}

	async fn set_block_executed(
		&self,
		block_number: BlockNumber,
		cluster_address: &Address,
	) -> Result<(), Error> {
		let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
		match self
			.session
			.query(
				"UPDATE block_meta_info SET block_executed = ? WHERE cluster_address = ? and block_number = ?;",
				(&true, cluster_address, &block_number),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Failed to update block_meta_info - {}", e)),
		}
	}
}
