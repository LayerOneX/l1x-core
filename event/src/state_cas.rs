use anyhow::{anyhow, Error};
use async_trait::async_trait;

use db_traits::{base::BaseState, event::EventState};
use ethereum_types::{Bloom, H160, H256, U256};
use log::debug;
use primitives::*;
use scylla::{_macro_internal::CqlValue, frame::value::CqlTimestamp, Session};
use std::{
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};
use system::event::Event;
use types::eth::{
	block::BlockNumber,
	bloom_tree::create_logs_bloom,
	filter::{Filter, VariadicValue},
	log::Log,
};

pub struct StateCas {
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<Event> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS event (
                    block_number Bigint,
                    transaction_hash Varchar,
                    event_type tinyint,
                    contract_address Varchar,
                    event_data blob,
                    topic0 Varchar,
                    topic1 Varchar,
                    topic2 Varchar,
                    topic3 Varchar,
                    timestamp Timestamp,
                    PRIMARY KEY (timestamp, transaction_hash)
                );",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Create table failed: {e:?}")),
		};

		// WHERE event_type = 'EVM'
		// AND contract_address = '0xasdf...'
		// AND topic0 = '0xasdf...'
		// AND block_number >= 10
		// AND block_number < 1245

		// Add secondary indexes to allow filtering on non primary key columns
		match self
			.session
			.query("CREATE INDEX IF NOT EXISTS ON event (event_type);", &[])
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed creating index on event_type: {e:?}")),
		}

		match self
			.session
			.query("CREATE INDEX IF NOT EXISTS ON event (contract_address);", &[])
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed creating index on contract_address: {e:?}")),
		}

		match self.session.query("CREATE INDEX IF NOT EXISTS ON event (topic0);", &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed creating index on topic0: {e:?}")),
		}

		match self.session.query("CREATE INDEX IF NOT EXISTS ON event (topic1);", &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed creating index on topic1: {e:?}")),
		}

		match self.session.query("CREATE INDEX IF NOT EXISTS ON event (topic2);", &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed creating index on topic2: {e:?}")),
		}

		match self.session.query("CREATE INDEX IF NOT EXISTS ON event (topic3);", &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed creating index on topic3: {e:?}")),
		}

		match self
			.session
			.query("CREATE INDEX IF NOT EXISTS ON event (block_number);", &[])
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed creating index on block_number: {e:?}")),
		}

		match self
			.session
			.query("CREATE INDEX IF NOT EXISTS ON event (transaction_hash);", &[])
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed creating index on transaction_hash: {e:?}")),
		}

		Ok(())
	}

	async fn create(&self, event: &Event) -> Result<(), Error> {
		let milliseconds: i64 = SystemTime::now()
			.duration_since(UNIX_EPOCH)?
			.as_millis()
			.try_into()
			.unwrap_or(i64::MAX);
		//let timestamp = Duration::milliseconds(milliseconds);
		// let timestamp_u128: ScalarBig = timestamp.to_byte_array_le(ScalarBig::default());
		let block_number: i64 = i64::try_from(event.block_number).unwrap_or(i64::MAX);
		let transaction_hash = hex::encode(event.transaction_hash);
		/*let event_type: String = match event.event_type {
			EventType::L1XVM => String::from("L1XVM"),
			EventType::EVM => String::from("EVM"),
		};*/
		let contract_address = hex::encode(event.contract_address);

		let mut topic0: Vec<u8> = vec![];
		let mut topic1: Vec<u8> = vec![];
		let mut topic2: Vec<u8> = vec![];
		let mut topic3: Vec<u8> = vec![];

		if let Some(topics_vec) = &event.topics {
			for (i, topic) in topics_vec.iter().enumerate() {
				match i {
					0 => topic0.extend_from_slice(&topic.0),
					1 => topic1.extend_from_slice(&topic.0),
					2 => topic2.extend_from_slice(&topic.0),
					3 => topic3.extend_from_slice(&topic.0),
					_ => {},
				}
			}
		}

		let topic0 = hex::encode(topic0);
		let topic1 = hex::encode(topic1);
		let topic2 = hex::encode(topic2);
		let topic3 = hex::encode(topic3);

		let event_type = event.event_type;

		let query = "INSERT INTO event (block_number, transaction_hash, event_type, contract_address, event_data, topic0, topic1, topic2, topic3, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string();
		let values = (
			&block_number,
			transaction_hash.clone(),
			event_type,
			contract_address.clone(),
			&event.event_data,
			topic0.clone(),
			topic1.clone(),
			topic2.clone(),
			topic3.clone(),
			CqlTimestamp(milliseconds),
		);
		debug!("INSERT INTO event (block_number, transaction_hash, event_type, contract_address, event_data, topic0, topic1, topic2, topic3, timestamp) \nVALUES ({}, {}, {}, {}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?})", &block_number, transaction_hash, &event.event_type, contract_address, &event.event_data, topic0, topic1, topic2, topic3, CqlTimestamp(milliseconds));

		let result = self.session.query(query, values).await;

		match result {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Event: Create event failed - {}", e)),
		};
		Ok(())
	}

	async fn update(&self, _event: &Event) -> Result<(), Error> {
		// Implementation for update method
		todo!()
	}

	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match self.session.query(query, &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed to execute raw query: {}", e)),
		};
		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}
}

#[async_trait]
impl EventState for StateCas {
	async fn get_events(
		&self,
		transaction_hash: &TransactionHash,
		start_time_ms: i64, /* Pass 0 if you are calling for first time, for newer events pass
		                     * the last timestamp you received the events. */
	) -> Result<Vec<EventData>, Error> {
		// let timestamp_bytes: ScalarBig = timestamp.to_byte_array_le(ScalarBig::default());
		//let start_time_ms = Duration::milliseconds(start_time_ms);
		let query: &str = r#"
            SELECT event_data FROM event WHERE transaction_hash = ? AND timestamp > ? ALLOW FILTERING;
            "#;

		let values = (hex::encode(transaction_hash), CqlTimestamp(start_time_ms));

		// println!("SELECT event_data FROM event WHERE transaction_hash = '{}' AND timestamp >
		// ? ALLOW FILTERING;", hex::encode(transaction_hash));

		let query_result = match self.session.query(query, values).await {
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Query failed - {}", e)),
		};

		let (event_data_idx, _) = query_result
			.get_column_spec("event_data")
			.ok_or_else(|| anyhow!("No event column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		let mut events = Vec::new();

		for row in rows {
			let event_data: EventData = if let Some(event_data_value) = &row.columns[event_data_idx]
			{
				if let CqlValue::Blob(event_data) = event_data_value {
					event_data.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
				} else {
					return Err(anyhow!("Unable to convert to EventData type"));
				}
			} else {
				return Err(anyhow!("Unable to read event_data column"));
			};
			events.push(event_data);
		}
		Ok(events)
	}

	async fn get_all_events(
		&self,
		transaction_hash: &TransactionHash,
	) -> Result<Vec<EventData>, Error> {
		let transaction_hash = hex::encode(transaction_hash);
		let query_result = match self
			.session
			.query(
				"SELECT
                        block_number,
                        transaction_hash,
                        event_type,
                        contract_address,
                        event_data,
                        topic0,
                        topic1,
                        topic2,
                        topic3,
                        timestamp
                    FROM event WHERE transaction_hash = ?;",
				(&transaction_hash,),
			)
			.await
		{
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Invalid address: {}", e)),
		};
		let mut logs: Vec<EventData> = vec![];
		let mut log_index = 0;
		let mut transaction_log_index = 0;
		//info!("EVENT QUERY RESULT: {:?}", query_result);
		if let Some(rows) = query_result.rows {
			//info!("EVENT QUERY RESULT len: {:?}", rows.len());
			for row in rows {
				//info!("row: {:?}", row);
				let (
					block_number,
					transaction_hash,
					_event_type,
					contract_address,
					event_data,
					topic0,
					topic1,
					topic2,
					topic3,
					_timestamp,
				): (
					i64,
					String,
					EventType,
					String,
					Vec<u8>,
					String,
					String,
					String,
					String,
					CqlTimestamp,
				) = row.into_typed::<(
					i64,
					String,
					EventType,
					String,
					Vec<u8>,
					String,
					String,
					String,
					String,
					CqlTimestamp,
				)>()?;
				/*info!(
					"row data: {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, {:?}, ",
					contract_address,
					block_number,
					transaction_hash,
					event_type,
					contract_address,
					event_data,
					topic0,
					topic1,
					topic2,
					topic3,
					timestamp,
				);*/
				// Format contract address
				let address_vec = hex::decode(contract_address)?;
				let mut address_bytes: [u8; 20] = [0; 20];
				address_bytes.copy_from_slice(&address_vec);

				// Format topics
				let mut topics_formatted: Vec<H256> = vec![];
				for topic in [topic0, topic1, topic2, topic3].iter() {
					if !topic.is_empty() {
						let topic_vec = hex::decode(topic)?;
						let mut topic_bytes: [u8; 32] = [0; 32];
						topic_bytes.copy_from_slice(&topic_vec);
						topics_formatted.push(H256(topic_bytes));
					}
				}

				// Format block number
				let block_number: U256 = U256::from(block_number);

				// Format transaction hash
				let mut tx_hash_bytes: [u8; 32] = [0; 32];
				tx_hash_bytes.copy_from_slice(hex::decode(transaction_hash)?.as_ref());

				let bloom_data = vec![
					(H160(address_bytes), topics_formatted.clone()),
					// Add more logs as needed
				];
				let bloom: Bloom = create_logs_bloom(bloom_data);
				let log = Log {
					address: H160(address_bytes),
					topics: topics_formatted,
					data: event_data.into(),
					block_hash: None,
					block_number: Some(block_number),
					transaction_hash: Some(H256(tx_hash_bytes)),
					transaction_index: None,
					log_index: Some(U256::from(log_index as u64)),
					transaction_log_index: Some(U256::from(transaction_log_index as u64)),
					logs_bloom: Some(bloom),
					removed: false,
				};
				let json_string = serde_json::to_string(&log)?;
				//info!("json_string: {:?}", json_string.clone());
				// Convert the JSON string to bytes (Vec<u8>).
				let bytes: Vec<u8> = json_string.into_bytes();
				logs.push(bytes);
				log_index += 1;
				transaction_log_index += 1;
			}
		}

		// debug!("Logs: {:?}", logs);

		Ok(logs)
	}

	/// Get filtered EVM events
	async fn get_filtered_events(&self, filter: Filter) -> Result<Vec<Log>, Error> {
		// Starting query
		let mut query = "SELECT contract_address, event_data, topic0, topic1, topic2, topic3, block_number, transaction_hash FROM event WHERE event_type = 1".to_string();

		// Holds dynamic SQL query conditions
		let mut logics: Vec<String> = vec![];

		if let Some(from) = filter.from_block {
			match from {
				BlockNumber::Num(n) => {
					let sql = format!("block_number >= {}", n);
					logics.push(sql);
				},
				_ => {
					let sql = "block_number >= 0".to_string();
					logics.push(sql);
				},
			}
		}

		if let Some(to) = filter.to_block {
			match to {
				BlockNumber::Num(n) => {
					let sql = format!("block_number <= {}", n);
					logics.push(sql);
				},
				_ => {
					// TODO: handle other cases later
					// let sql = format!("block_number <= {}", current_block_number);
					// logics.push(sql);
					return Err(anyhow!("Filtering by block number is only supported for the Number variant right now (ethers_core::types::BlockNumber::Number)"));
				},
			}
		}

		// Handle filtering for events by smart contract address
		match filter.address {
			VariadicValue::Single(addr) => {
				let addr = hex::encode(addr.0);
				let sql = format!("contract_address = '{}'", addr);
				logics.push(sql);
			},
			VariadicValue::Multiple(addr_arr) =>
				if !addr_arr.is_empty() {
					let conditions: Vec<String> = addr_arr
						.iter()
						.map(|addr| format!("contract_address = '{}'", hex::encode(addr.0)))
						.collect();
					let sql = if addr_arr.len() == 1 {
						conditions[0].clone()
					} else {
						format!("({})", conditions.join(" OR "))
					};
					logics.push(sql);
				},
			VariadicValue::Null => {
				// Handle null case if necessary
			},
		}

		// Handle filtering for events by topic
		for (i, maybe_topic) in filter.topics.iter().enumerate() {
			if let Some(topic) = maybe_topic {
				match topic {
					VariadicValue::Single(maybe_topic) =>
						if let Some(topic) = maybe_topic {
							let topic = hex::encode(topic.0);
							let sql = format!("topic{} = '{}'", i, topic);
							logics.push(sql);
						},
					VariadicValue::Multiple(_topic_arr) => {
						// if topic_arr.len() == 1 {
						//     let sql = format!("topic{} = '{}'", i, hex::encode(topic_arr[0]));
						//     logics.push(sql);
						// } else if topic_arr.len() > 1 {
						//     let mut sql = "(".to_string();
						//     for (i, topic) in topic_arr.iter().enumerate() {
						//         let topic = hex::encode(topic.0);
						//         let mut condition = format!("topic{} = '{}'", i, topic);
						//         if i != topic_arr.len() - 1 {
						//             condition.push_str(" OR ");
						//         }
						//         sql.push_str(&condition);
						//     }
						//     sql.push_str(")\n");
						// }
						return Err(anyhow!("Filtering by topic array is not supported yet"));
					},
					VariadicValue::Null => {
						return Err(anyhow!("Anonymous filtering is not supported yet"));
					},
				};
			}
		}

		if !logics.is_empty() {
			query.push_str("\nAND ");
		}

		// Dynamically generate SQL query
		for (i, condition) in logics.iter().enumerate() {
			query.push_str(condition);

			if i != logics.len() - 1 {
				query.push_str("\nAND ");
			} else {
				query.push_str(" ALLOW FILTERING;");
			}
		}

		debug!("Dynamically generated SQL query: {query}");

		// Execute the query
		let query_result = match self.session.query(query, &[]).await {
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Query failed - {}", e)),
		};
		//debug!("EVENT QUERY RESULT: {:?}", query_result);

		// Loop through the query result and format the logs
		let mut log_index = 0;
		let mut transaction_log_index = 0;
		let mut logs: Vec<Log> = vec![];
		if let Some(rows) = query_result.rows {
			for row in rows {
				debug!("row: {:?}", row);

				let (
					contract_address,
					event_data,
					topic0,
					topic1,
					topic2,
					topic3,
					block_number,
					transaction_hash,
				): (String, Vec<u8>, String, String, String, String, i64, String) = row.into_typed::<(
					String,
					Vec<u8>,
					String,
					String,
					String,
					String,
					i64,
					String,
				)>()?;

				// Format contract address
				let address_vec = hex::decode(contract_address)?;
				let mut address_bytes: [u8; 20] = [0; 20];
				address_bytes.copy_from_slice(&address_vec);

				// Format topics
				let mut topics_formatted: Vec<H256> = vec![];
				for topic in [topic0, topic1, topic2, topic3].iter() {
					if !topic.is_empty() {
						let topic_vec = hex::decode(topic)?;
						let mut topic_bytes: [u8; 32] = [0; 32];
						topic_bytes.copy_from_slice(&topic_vec);
						topics_formatted.push(H256(topic_bytes));
					}
				}

				// match topics {
				//     Some(topics_vec) => {
				//         debug!("TOPICS_vec_LEN: {}, TOPICS_vec: {:?}", topics_vec.len(),
				// topics_vec);

				//         for topic in topics_vec.chunks(32) {
				//             let mut topic_bytes: [u8; 32] = [0; 32];
				//             topic_bytes.copy_from_slice(&topic);
				//             topics_formatted.push(H256(topic_bytes));
				//         }
				//     }
				//     None => {}
				// }

				// Format block number
				let block_number: U256 = U256::from(block_number);

				// Format transaction hash
				let mut tx_hash_bytes: [u8; 32] = [0; 32];
				tx_hash_bytes.copy_from_slice(hex::decode(transaction_hash)?.as_ref());

				let bloom_data = vec![
					(H160(address_bytes), topics_formatted.clone()),
					// Add more logs as needed
				];
				let bloom: Bloom = create_logs_bloom(bloom_data);
				let log = Log {
					address: H160(address_bytes),
					topics: topics_formatted,
					data: event_data.into(),
					block_hash: None,
					block_number: Some(block_number),
					transaction_hash: Some(H256(tx_hash_bytes)),
					transaction_index: None,
					log_index: Some(U256::from(log_index as u64)),
					transaction_log_index: Some(U256::from(transaction_log_index as u64)),
					logs_bloom: Some(bloom),
					removed: false,
				};
				logs.push(log);
				log_index += 1;
				transaction_log_index += 1;
			}
		}

		// debug!("Logs: {:?}", logs);

		Ok(logs)
	}

	async fn is_valid_event(&self, transaction_hash: &TransactionHash) -> Result<bool, Error> {
		let transaction_hash = hex::encode(transaction_hash);
		let query_result = match self
			.session
			.query(
				"SELECT COUNT(*) AS count FROM event WHERE transaction_hash = ?;",
				(&transaction_hash,),
			)
			.await
		{
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Event query failed: {}", e)),
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
					return Err(anyhow!("Unable to convert to BigInt type"));
				}
			}
		}
		Ok(false)
	}
}
