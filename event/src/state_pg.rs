use anyhow::{anyhow, Error};
use async_trait::async_trait;
use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::NaiveDateTime;
use db::postgres::{
	pg_models::{NewEvent, QueryEvent},
	postgres::{PgConnectionType, PostgresDBConn},
	schema,
};
use db_traits::{base::BaseState, event::EventState};
use diesel::{
	self, internal::table_macro::BoxedSelectStatement, prelude::*, QueryResult,
};
use ethereum_types::{Bloom, H160, H256, U256};

use primitives::{Address, EventData, TransactionHash};
use system::event::Event;
use types::eth::{
	block::BlockNumber,
	bloom_tree::create_logs_bloom,
	filter::{Filter, VariadicValue},
	log::Log,
};
use util::{
    convert::convert_to_big_decimal_block_number,
    generic::{convert_to_naive_datetime, current_timestamp_in_millis},
};

pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}

#[async_trait]
impl<'a> BaseState<Event> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, evnt: &Event) -> Result<(), Error> {
		use db::postgres::schema::event::dsl::*;
		let timestap = current_timestamp_in_millis()?;
		let block_numbr = convert_to_big_decimal_block_number(evnt.block_number);
		let transaction_hsh = hex::encode(evnt.transaction_hash);
		let contract_addrss = hex::encode(evnt.contract_address);
		let event_dat = hex::encode(evnt.event_data.clone());

		let mut topc0: Vec<u8> = vec![];
		let mut topc1: Vec<u8> = vec![];
		let mut topc2: Vec<u8> = vec![];
		let mut topc3: Vec<u8> = vec![];

		if let Some(topics_vec) = &evnt.topics {
			for (i, topic) in topics_vec.iter().enumerate() {
				match i {
					0 => topc0.extend_from_slice(&topic.0),
					1 => topc1.extend_from_slice(&topic.0),
					2 => topc2.extend_from_slice(&topic.0),
					3 => topc3.extend_from_slice(&topic.0),
					_ => {},
				}
			}
		}

		let topc0 = hex::encode(topc0);
		let topc1 = hex::encode(topc1);
		let topc2 = hex::encode(topc2);
		let topc3 = hex::encode(topc3);

		let new_event = NewEvent {
			transaction_hash: transaction_hsh,
			timestamp: Some(convert_to_naive_datetime(timestap)),
			block_number: Some(block_numbr),
			contract_address: Some(contract_addrss),
			event_data: Some(event_dat),
			event_type: Some(evnt.event_type as i16),
			topic0: Some(topc0),
			topic1: Some(topc1),
			topic2: Some(topc2),
			topic3: Some(topc3),
		};
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) =>
				diesel::insert_into(event).values(new_event).execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(schema::event::table)
				.values(new_event)
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn update(&self, _event: &Event) -> Result<(), Error> {
		// Implementation for update method
		todo!()
	}

	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::sql_query(query).execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) =>
				diesel::sql_query(query).execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}
}

#[async_trait]
impl<'a> EventState for StatePg<'a> {
	async fn get_events(
		&self,
		transaction_hash: &TransactionHash,
		start_time_ms: i64,
	) -> Result<Vec<EventData>, Error> {
		let encode_trx_hash = hex::encode(transaction_hash);

		// Start with a boxed query
		let mut query: BoxedSelectStatement<_, _, _> = schema::event::table.into_boxed();

		// Always apply the transaction_hash filter
		query = query.filter(schema::event::dsl::transaction_hash.eq(encode_trx_hash));

		// todo!: Remove deprecated version
		// Conditionally apply the timestamp filter
		if start_time_ms != 0 {
			#[allow(deprecated)]
			let start_time_naive = NaiveDateTime::from_timestamp(
				start_time_ms / 1000,
				(start_time_ms % 1000) as u32 * 1_000_000,
			);
			query = query.filter(schema::event::dsl::timestamp.eq(start_time_naive));
		}

		// Execute the query

		let res: QueryResult<Vec<QueryEvent>> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => query.load(*conn.lock().await),
			PgConnectionType::PgConn(conn) => query.load(&mut *conn.lock().await),
		};

		// Process the query results
		res.map_err(|e| anyhow::anyhow!("Diesel query failed: {}", e))
			.and_then(|results| {
				results
					.into_iter()
					.map(|query_result| {
						hex::decode(query_result.event_data.unwrap().as_str())
							.map_err(|e| anyhow::anyhow!("Failed to decode event data: {}", e))
					})
					.collect()
			})
	}

	async fn get_all_events(
		&self,
		transaction_hash: &TransactionHash,
	) -> Result<Vec<EventData>, Error> {
		let encoded_trx_hash = hex::encode(transaction_hash);

		let res: QueryResult<Vec<QueryEvent>> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::event::table
				.filter(schema::event::dsl::transaction_hash.eq(encoded_trx_hash))
				.load(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::event::table
				.filter(schema::event::dsl::transaction_hash.eq(encoded_trx_hash))
				.load(&mut *conn.lock().await),
		};

		let mut logs: Vec<EventData> = vec![];
		let mut transaction_log_index = 0;
		match res {
			Ok(results) => {
				for (log_index, query_results) in results.into_iter().enumerate() {
					let mut address_bytes: Address = [0; 20];
					let mut tx_hash_bytes = [0; 32];

					let event_data = hex::decode(query_results.event_data.unwrap().as_str())?;
					let tx = hex::decode(query_results.transaction_hash.as_str())?;

					if tx.len() == 32 {
						let mut array = [0u8; 32];
						array.copy_from_slice(&tx);
						tx_hash_bytes = array;
					}

					let contract = hex::decode(
						query_results.contract_address.clone().unwrap_or_else(|| "".to_string()),
					)?;

					if contract.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&contract);
						address_bytes = array;
					}

					let mut topics_formatted: Vec<H256> = vec![];
					for topic in [
						query_results.topic0.unwrap(),
						query_results.topic1.unwrap(),
						query_results.topic2.unwrap(),
						query_results.topic3.unwrap(),
					]
					.iter()
					{
						if !topic.is_empty() {
							let topic_vec = hex::decode(topic)?;
							let mut topic_bytes: [u8; 32] = [0; 32];
							topic_bytes.copy_from_slice(&topic_vec);
							topics_formatted.push(H256(topic_bytes));
						}
					}

					let block_number: u64 = match query_results.block_number {
						Some(decimal) => match decimal.to_u64() {
							Some(to_u64_val) => to_u64_val,
							None =>
								return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
						},
						None => return Err(anyhow::anyhow!("Block number is None")),
					};
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
						block_number: Some(block_number.into()),
						transaction_hash: Some(H256(tx_hash_bytes)),
						transaction_index: None,
						log_index: Some(U256::from(log_index as u64)),
						logs_bloom: Some(bloom),
						transaction_log_index: Some(U256::from(transaction_log_index as u64)),
						removed: false,
					};

					let json_string = serde_json::to_string(&log)?;
					// Convert the JSON string to bytes (Vec<u8>).
					let bytes: Vec<u8> = json_string.into_bytes();
					logs.push(bytes);
					transaction_log_index += 1;
				}
				Ok(logs)
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	/// Get filtered EVM events
	async fn get_filtered_events(&self, filter: Filter) -> Result<Vec<Log>, Error> {
		let mut query = schema::event::dsl::event.into_boxed();

		// Handle block range
		let (from_block, to_block) = (filter.from_block, filter.to_block);
		if let Some(BlockNumber::Num(n)) = from_block {
			let n = u128::from(n);
			let block_number: BigDecimal = convert_to_big_decimal_block_number(n);
			query = query.filter(schema::event::dsl::block_number.ge(block_number));
		} else {
			let block_number: BigDecimal = convert_to_big_decimal_block_number(0);
			query = query.filter(schema::event::dsl::block_number.ge(block_number));
		}
		if let Some(BlockNumber::Num(n)) = to_block {
			let n = u128::from(n);
			let block_number: BigDecimal = convert_to_big_decimal_block_number(n);
			query = query.filter(schema::event::dsl::block_number.le(block_number));
		} else {
			//query = query.filter(schema::event::dsl::block_number.ge(0));
			return Err(anyhow!("Filtering by block number is only supported for the Number variant right now (ethers_core::types::BlockNumber::Number)"));
		}

		// Filtering by block hash is not yet supported
		// TODO: Add support for filtering by block hash

		// Handle filtering for events by smart contract address
		let _address = filter.address;

		// Handle filtering for events by topic
		for (i, maybe_topic) in filter.topics.iter().enumerate() {
			if let Some(topic) = maybe_topic {
				match topic {
					VariadicValue::Single(maybe_topic) =>
						if let Some(topic) = maybe_topic {
							let topic = hex::encode(topic.0);
							match i {
								0 => query = query.filter(schema::event::dsl::topic0.eq(topic)),
								1 => query = query.filter(schema::event::dsl::topic1.eq(topic)),
								2 => query = query.filter(schema::event::dsl::topic2.eq(topic)),
								3 => query = query.filter(schema::event::dsl::topic3.eq(topic)),
								_ => {},
							}
						},
					VariadicValue::Multiple(_topic_arr) => {
						return Err(anyhow!("Filtering by topic array is not supported yet"));
					},
					VariadicValue::Null => {
						return Err(anyhow!("Anonymous filtering is not supported yet"));
					},
				};
			}
		}

		// Execute the query
		let query_result: Vec<QueryEvent> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => query
				.load(*conn.lock().await)
				.map_err(|e| anyhow!("Failed to execute query: {}", e)),
			PgConnectionType::PgConn(conn) => query
				.load(&mut *conn.lock().await)
				.map_err(|e| anyhow!("Failed to execute query: {}", e)),
		}?;

		let mut logs: Vec<Log> = vec![];
		let mut transaction_log_index = 0;
		for (log_index, event) in query_result.into_iter().enumerate() {
			let event_data = hex::decode(event.event_data.unwrap().as_str())?;
			let transaction_hash = hex::decode(event.transaction_hash.as_str())?;

			let transaction_hash: [u8; 32] = transaction_hash
				.try_into()
				.map_err(|_| anyhow!("transaction_hash must be 32 bytes"))?;

			let mut topics_formatted: Vec<H256> = vec![];
			for topic in [
				event.topic0.unwrap(),
				event.topic1.unwrap(),
				event.topic2.unwrap(),
				event.topic3.unwrap(),
			]
			.iter()
			{
				if !topic.is_empty() {
					let topic_vec = hex::decode(topic)?;
					let mut topic_bytes: [u8; 32] = [0; 32];
					topic_bytes.copy_from_slice(&topic_vec);
					topics_formatted.push(H256(topic_bytes));
				}
			}
			let contract_address =
				hex::decode(event.contract_address.clone().unwrap_or_else(|| "".to_string()))?;
			let block_number: u64 = match event.block_number {
				Some(decimal) => match decimal.to_u64() {
					Some(to_u64_val) => to_u64_val,
					None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
				},
				None => return Err(anyhow::anyhow!("Block number is None")),
			};
			let contract_address: [u8; 20] = contract_address
				.try_into()
				.map_err(|_| anyhow!("Contract address must be 20 bytes"))?;
			let bloom_data = vec![
				(H160(contract_address), topics_formatted.clone()),
				// Add more logs as needed
			];
			let bloom: Bloom = create_logs_bloom(bloom_data);

			let log = Log {
				address: H160(contract_address),
				topics: topics_formatted,
				data: event_data.into(),
				block_hash: None,
				block_number: Some(block_number.into()),
				transaction_hash: Some(H256(transaction_hash)),
				transaction_index: None,
				log_index: Some(U256::from(log_index as u64)),
				logs_bloom: Some(bloom),
				transaction_log_index: Some(U256::from(transaction_log_index as u64)),
				removed: false,
			};
			logs.push(log);
			transaction_log_index += 1;
		}

		Ok(logs)
	}

	async fn is_valid_event(&self, _transaction_hash: &TransactionHash) -> Result<bool, Error> {
		let encode_trx_hash = hex::encode(_transaction_hash);

		use db::postgres::schema::event::dsl::*;
		let res = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => event
				.filter(transaction_hash.eq(encode_trx_hash))
				.load::<QueryEvent>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => event
				.filter(transaction_hash.eq(encode_trx_hash))
				.load::<QueryEvent>(&mut *conn.lock().await),
		}?;

		Ok(!res.is_empty())
	}
}
