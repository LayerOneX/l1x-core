use anyhow::{anyhow, Context, Error};
use chrono::Duration;
use db::{
    cassandra::DatabaseManager,
    utils::{FromByteArray, ToByteArray},
};
use l1x_rpc::rpc_model::{self, TransactionResponse};
use log::{debug, info};
use primitives::{arithmetic::ScalarBig, *};
use scylla::{_macro_internal::CqlValue, frame::value::CqlTimestamp, Session};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use system::{
    account::Account,
    block::{Block, BlockResponse, BlockType},
    block_header::BlockHeader,
    chain_state::ChainState,
    transaction::Transaction,
    transaction_receipt::TransactionReceiptResponse,
};
use util::{
    convert::{bytes_to_address, to_proto_transaction},
    generic::{get_cassandra_timestamp, current_timestamp_in_micros},
};

pub struct BlockState {
    pub session: Arc<Session>,
}

impl BlockState {
    pub async fn new() -> Result<Self, Error> {
        let db_session = DatabaseManager::get_session().await?;
        let block_state = BlockState { session: db_session.clone() };
        block_state.create_table().await?;
        Ok(block_state)
    }

    pub async fn create_table(&self) -> Result<(), Error> {
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
            Err(_) => return Err(anyhow!("Create table failed")),
        };
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
            Err(_) => return Err(anyhow!("Create table failed")),
        };
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
                    PRIMARY KEY (transaction_hash, block_number)
                )   WITH CLUSTERING ORDER BY (block_number ASC);",
                &[],
            )
            .await
        {
            Ok(_) => {},
            Err(_) => return Err(anyhow!("Create table failed")),
        };
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
            Err(_) => return Err(anyhow!("Create table failed")),
        };
        Ok(())
    }

    pub async fn store_block_head(
        &self,
        cluster_address: &Address,
        block_number: BlockNumber,
        block_hash: BlockHash,
    ) -> Result<(), Error> {
        // let block_number_bytes: ScalarBig = block_number.to_byte_array_le(ScalarBig::default());
        let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
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

    pub async fn update_block_head(
        &self,
        cluster_address: Address,
        big_block_number: BlockNumber,
        block_hash: BlockHash,
    ) -> Result<(), Error> {
        // let block_number_bytes: ScalarBig = block_number.to_byte_array_le(ScalarBig::default());
        let block_number: i64 = i64::try_from(big_block_number).unwrap_or(i64::MAX);
        if !self.is_block_head(&cluster_address).await? {
            self.store_block_head(&cluster_address, big_block_number, block_hash).await
        } else {
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
                    self.store_block_head(&cluster_address, big_block_number, block_hash)
                        .await
                }
            }
        }
    }

    pub async fn load_chain_state(&self, cluster_address: Address) -> Result<ChainState, Error> {
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

    pub async fn block_head(&self, cluster_address: Address) -> Result<Block, Error> {
        let chain_state = self.load_chain_state(cluster_address).await?;
        self.load_block(chain_state.block_number, &cluster_address).await
    }

    pub async fn block_head_header(&self, cluster_address: Address) -> Result<BlockHeader, Error> {
        let chain_state = self.load_chain_state(cluster_address).await?;
        self.load_block_header(chain_state.block_number, &cluster_address).await
    }

    pub async fn store_block(&self, block: Block) -> Result<(), Error> {
        // let block_number_bytes: ScalarBig =
        //     block.block_number.to_byte_array_le(ScalarBig::default());
        let block_number = i64::try_from(block.block_header.block_number).unwrap_or(i64::MAX);
        let num_transactions = u32::try_from((&block.transactions).len()).unwrap_or(u32::MAX);

        println!(
            "ℹ️ STORING BLOCK #{}, CLUSTER ADDRESS 0x{:?}",
            block_number, block.block_header.cluster_address
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
        for transaction in transactions {
            self.store_transaction(
                block.block_header.block_number.clone(),
                block.block_header.block_hash.clone(),
                transaction,
            )
                .await?;
        }

        self.update_block_head(
            block.block_header.cluster_address,
            block.block_header.block_number,
            block.block_header.block_hash.clone(),
        )
            .await?;

        Ok(())
    }

    pub async fn store_transaction(
        &self,
        block_number: BlockNumber,
        block_hash: BlockHash,
        transaction: Transaction,
    ) -> Result<(), Error> {
        // let block_number_bytes: ScalarBig = block_number.to_byte_array_le(ScalarBig::default());
        let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
        let fee_used_bytes: ScalarBig =
            transaction.fee_limit.to_byte_array_le(ScalarBig::default());
        let transaction_bytes: Vec<u8> = bincode::serialize(&transaction)?;
        let transaction_hash = transaction.custom_hash.unwrap().0?;
        let from_address = Account::address(&transaction.verifying_key)?;
        let timestamp = get_cassandra_timestamp()?;

        match self
            .session
            .query(
                "INSERT INTO block_transaction (transaction_hash, block_number, transaction, block_hash, fee_used, from_address, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?);",
                (&transaction_hash, &block_number, &transaction_bytes, block_hash, fee_used_bytes, from_address, timestamp),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Store transaction failed - {}", e)),
        }
    }

    // FIXME: mitigate "ALLOW FILTERING" with partition keys/secondary indexes
    pub async fn get_transaction_count(
        &self,
        verifying_key: &Address,
        block_number: Option<BlockNumber>,
    ) -> Result<u64, Error> {
        let (tx_count,) = match block_number {
            Some(block_number) => {
                // let block_number: ScalarBig = block_number.to_byte_array_le(ScalarBig::default());
                let block_number = i64::try_from(block_number).unwrap_or(i64::MAX);
                self.session.query("SELECT COUNT(*) FROM block_transaction WHERE from_address = ? AND block_number <= ? ALLOW FILTERING;", (verifying_key, &block_number)).await?.single_row()?.into_typed::<(i64,)>()?
            }
            None => self
                .session
                .query(
                    "SELECT COUNT(*) FROM block_transaction WHERE from_address = ? ALLOW FILTERING;",
                    (verifying_key,),
                )
                .await?
                .single_row()?
                .into_typed::<(i64,)>()?,
        };
        Ok(tx_count as u64)
    }

    pub async fn load_block(
        &self,
        block_number: BlockNumber,
        cluster_address: &Address,
    ) -> Result<Block, Error> {
        let block_header: BlockHeader =
            self.load_block_header(block_number, cluster_address).await?;
        let transactions: Vec<Transaction> = self.load_transactions(block_number).await?;
        Ok(Block { block_header, transactions })
    }

    pub async fn load_block_response(
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

    pub async fn store_block_header(&self, block_header: BlockHeader) -> Result<(), Error> {
        let block_number = i64::try_from(block_header.block_number).unwrap_or(i64::MAX);
        let block_hash = block_header.block_hash;
        let parent_hash = block_header.block_hash;
        let timestamp = current_timestamp_in_micros()?; // Convert the duration to microseconds
        let milliseconds = i64::try_from(timestamp).unwrap_or(i64::MAX);
        //let timestamp = Duration::microseconds(milliseconds);
        let block_type: i8 = match block_header.block_type {
            BlockType::L1XTokenBlock => 0,
            BlockType::L1XContractBlock => 1,
            BlockType::XTalkTokenBlock => 2,
            BlockType::XTalkContractBlock => 3,
            BlockType::SuperBlock => 4,
        };
        match self.session.query(
            "INSERT INTO block_header (block_number, block_hash, parent_hash, timestamp, block_type, num_transactions) VALUES (?, ?, ?, ?, ?, ?);",
            (&block_number, &block_hash, &parent_hash, &CqlTimestamp(milliseconds), &block_type, block_header.num_transactions as i32),
        )
            .await {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Storing block header failed: {}", e)),
        }
    }

    pub async fn load_block_header(
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
				))
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
                        block_version
                    ) = row.into_typed::<(i64, [u8; 32], i8, i32, [u8; 32], i64, [u8; 32], i32)>()?;

                    let header = BlockHeader {
                        block_number: block_number.try_into().unwrap_or(BlockNumber::MIN),
                        block_hash,
                        parent_hash,
                        block_type: match block_type {
                            0 => BlockType::L1XTokenBlock,
                            1 => BlockType::L1XContractBlock,
                            2 => BlockType::XTalkTokenBlock,
                            3 => BlockType::XTalkContractBlock,
                            4 => BlockType::SuperBlock,
                            _ => BlockType::L1XTokenBlock,
                        },
                        cluster_address: *cluster_address,
                        num_transactions: num_transactions.try_into().unwrap_or(i32::MIN),
                        block_version: block_version.try_into().unwrap_or(u32::MIN),
                        state_hash,
                    };

                    Ok(header)
                },
                None => return Err(anyhow!("No row for block header #{} found", block_number)),
            }
        } else {
            Err(anyhow!("No vector of rows for block header #{} found", block_number))
        }
    }

    pub async fn load_transaction(
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
            return Ok(None)
        } else {
            let transaction: Transaction = if let Some(row) = rows.get(0) {
                if let Some(transaction_value) = &row.columns[transaction_idx] {
                    if let CqlValue::Blob(transaction) = transaction_value {
                        bincode::deserialize(transaction)
                            .map_err(|_| anyhow!("Unable to deserialize to Transaction type"))?
                    } else {
                        return Err(anyhow!("Unable to convert to Transaction type"))
                    }
                } else {
                    return Err(anyhow!("Unable to read transaction column"))
                }
            } else {
                return Err(anyhow!("block_state : block_header_data 484: Unable to read row"))
            };
            Ok(Some(transaction))
        }
    }

    pub async fn load_transaction_receipt(
        &self,
        transaction_hash: TransactionHash,
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
				))
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
                    ) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, i64, Vec<u8>)>()?;

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
                        timestamp,
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

    pub async fn load_transaction_receipt_by_address(
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

        // let mut transaction_receipt_responses = Vec::new();

        // for row in rows {
        //     let transaction: Transaction =
        //         if let Some(transaction_value) = &row.columns[transaction_idx] {
        //             if let CqlValue::Blob(transaction) = transaction_value {
        //                 bincode::deserialize(transaction)
        //                     .map_err(|_| anyhow!("Unable to deserialize to Transaction type"))?
        //             } else {
        //                 return Err(anyhow!("Unable to convert to Transaction type"));
        //             }
        //         } else {
        //             return Err(anyhow!("Unable to read transaction column"));
        //         };

        //     let block_number: BlockNumber =
        //         if let Some(block_number_value) = &row.columns[block_number_idx] {
        //             if let CqlValue::Blob(block_number) = block_number_value {
        //                 u128::from_byte_array(block_number)
        //             } else {
        //                 return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"));
        //             }
        //         } else {
        //             return Err(anyhow!("Unable to read block_header_data column"));
        //         };

        //     let block_hash: BlockHash = if let Some(block_hash_value) =
        // &row.columns[block_hash_idx]     {
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
        //     };

        //     let fee_used: Balance = if let Some(fee_used_value) = &row.columns[fee_used_idx] {
        //         if let CqlValue::Blob(fee_used) = fee_used_value {
        //             u128::from_byte_array(fee_used)
        //         } else {
        //             return Err(anyhow!("Unable to convert to bytes from CqlValue::Blob"));
        //         }
        //     } else {
        //         return Err(anyhow!("Unable to read fee_used column"));
        //     };

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
                ) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, i64, Vec<u8>)>()?;

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
                    timestamp,
                    from: bytes_to_address(from_address)?,
                };

                transactions.push(txs);
            }
        } else {
            return Err(anyhow!("No vector of rows for address 0x{:?}", address))
        }

        Ok(transactions)
    }

    /// Load the latest x number of block headers
    pub async fn load_latest_block_headers(
        &self,
        num_blocks: u32,
        cluster_address: Address,
    ) -> Result<Vec<rpc_model::BlockHeader>, Error> {
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

        let mut block_headers: Vec<rpc_model::BlockHeader> = vec![];
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
                    block_version
                ) = row.into_typed::<(i64, [u8; 32], i8, i32, [u8; 32], i64, [u8; 32], i32)>()?;

                let header = rpc_model::BlockHeader {
                    block_number: block_number.try_into().unwrap_or(u64::MIN),
                    block_hash: hex::encode(block_hash),
                    parent_hash: hex::encode(parent_hash),
                    timestamp: timestamp.try_into().unwrap_or(u64::MIN),
                    block_type: block_type.into(),
                    cluster_address: hex::encode([0u8; 20]),
                    num_transactions: num_transactions.try_into().unwrap_or(u32::MIN),
                    block_version: block_version.try_into().unwrap_or(u32::MIN),
                    state_hash,
                };

                block_headers.push(header);
            }
        }
        Ok(block_headers)
    }

    /// Load the latest x number of transactions
    pub async fn load_latest_transactions(
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
                ) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, i64, Vec<u8>)>()?;

                let sys_transaction: Transaction = bincode::deserialize(&transaction)?;
                let rpc_transaction: rpc_model::Transaction =
                    to_proto_transaction(sys_transaction)?;
                let fee_used = u128::from_byte_array(&fee_used).to_string();
                let timestamp = u64::try_from(timestamp).unwrap_or(u64::MIN);

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
    pub async fn load_transactions(
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

        let mut transactions: Vec<Transaction> = vec![];
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
                ) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, i64, Vec<u8>)>()?;

                let sys_transaction: Transaction = bincode::deserialize(&transaction)?;
                transactions.push(sys_transaction);
            }
        }

        Ok(transactions)
    }

    /// Load the transactions for a given block number
    pub async fn load_transactions_response(
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
                ) = row.into_typed::<(Vec<u8>, i64, Vec<u8>, [u8; 32], Vec<u8>, i64, Vec<u8>)>()?;

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
                    timestamp,
                    from: bytes_to_address(from_address)?,
                };

                transactions.push(txs);
            }
        }

        Ok(transactions)
    }

    pub async fn is_block_head(&self, cluster_address: &Address) -> Result<bool, Error> {
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

        if let Some(row) = rows.get(0) {
            if let Some(count_value) = &row.columns[count_idx] {
                if let CqlValue::BigInt(count) = count_value {
                    return Ok(count > &0)
                } else {
                    return Err(anyhow!("Unable to convert to Nonce type"))
                }
            }
        }
        Ok(false)
    }

    pub async fn is_block_executed(
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

    pub async fn set_block_executed(
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