use anyhow::Error;
use async_trait::async_trait;
use bigdecimal::{BigDecimal, ToPrimitive};
use db::postgres::{
	pg_models::{NewBlockProposer, QueryBlockProposer},
	postgres::{PgConnectionType, PostgresDBConn},
	schema,
};
use db_traits::{base::BaseState, block_proposer::BlockProposerState};
use diesel::sql_types::Numeric;

use diesel::{self, prelude::*, QueryResult};
use primitives::{Address, Epoch};
use std::collections::HashMap;
use system::block_proposer::BlockProposer;
use util::convert::convert_to_big_decimal_epoch;

pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}

#[async_trait]
// impl BaseState<BlockProposer> for StatePg {
impl<'a> BaseState<BlockProposer> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, blk_proposer: &BlockProposer) -> Result<(), Error> {
		use db::postgres::schema::block_proposer::dsl::*;
		// Implementation for create method

		let new_blk_proposer = NewBlockProposer {
			address: hex::encode(blk_proposer.address),
			cluster_address: hex::encode(blk_proposer.cluster_address),
			epoch: convert_to_big_decimal_epoch(blk_proposer.epoch),
			selected_next: Some(false),
		};

		let res = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(block_proposer)
				.values(new_blk_proposer)
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(block_proposer)
				.values(new_blk_proposer)
				.execute(&mut *conn.lock().await),
		};

		match res {
			Ok(_result) => Ok(()),
			Err(e) => return Err(anyhow::anyhow!("Failed to create block proposer: {:?}", e)),
		}
	}

	async fn update(&self, blk_proposer: &BlockProposer) -> Result<(), Error> {
		use db::postgres::schema::block_proposer::dsl::*;

		let new_blk_proposer = NewBlockProposer {
			address: hex::encode(blk_proposer.address),
			cluster_address: hex::encode(blk_proposer.cluster_address),
			epoch: convert_to_big_decimal_epoch(blk_proposer.epoch),
			selected_next: Some(false), // Assuming you have this field in your BlockProposer
		};
		let res = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => {
				diesel::update(block_proposer.filter(epoch.eq(new_blk_proposer.clone().epoch)))
					.set((
						address.eq(new_blk_proposer.address.clone()),
						cluster_address.eq(new_blk_proposer.cluster_address.clone()),
						selected_next.eq(new_blk_proposer.selected_next),
					))
					.execute(*conn.lock().await)
			}
			PgConnectionType::PgConn(conn) => {
				diesel::update(block_proposer.filter(epoch.eq(new_blk_proposer.clone().epoch)))
					.set((
						address.eq(new_blk_proposer.address.clone()),
						cluster_address.eq(new_blk_proposer.cluster_address.clone()),
						selected_next.eq(new_blk_proposer.selected_next),
					))
					.execute(&mut *conn.lock().await)
			}
		};

		match res {
			Ok(_result) => Ok(()),
			Err(e) => return Err(anyhow::anyhow!("Failed to create block proposer: {:?}", e)),
		}
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
		Ok(())
	}
}
#[async_trait]
impl<'a> BlockProposerState for StatePg<'a> {
	async fn store_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
		address: Address,
	) -> Result<(), Error> {
		let new_blk_proposer = BlockProposer { cluster_address, address, epoch };
		self.create(&new_blk_proposer).await?;
		Ok(())
	}

	async fn load_selectors_block_epochs(
		&self,
		address: &Address,
	) -> Result<Option<Vec<Epoch>>, Error> {
		let encoded_address = hex::encode(address);
		let mut epoch_numbers = Vec::new();

		let res: QueryResult<Vec<QueryBlockProposer>> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::block_proposer::table
				.filter(schema::block_proposer::dsl::address.eq(encoded_address))
				.load(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::block_proposer::table
				.filter(schema::block_proposer::dsl::address.eq(encoded_address))
				.load(&mut *conn.lock().await),
		};

		match res {
			Ok(results) => {
				for query_results in results {
					let epoch_u64: u64 = match query_results.epoch.to_u64() {
						Some(u64_val) => u64_val,
						None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
					};
					epoch_numbers.push(epoch_u64);
				}
				Ok(Some(epoch_numbers))
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn update_selected_next(
		&self,
		selector_address: &Address,
		cluster_address: &Address,
	) -> Result<(), Error> {
		if let Some(epoch_numbers) = self.load_selectors_block_epochs(selector_address).await? {
			for epoch in epoch_numbers {
				let epoch = convert_to_big_decimal_epoch(epoch);
				let cluster_address = hex::encode(cluster_address);
				let address = hex::encode(selector_address);

				match &self.pg.conn {
					PgConnectionType::TxConn(conn) => {
						diesel::update(
							schema::block_proposer::table
								.filter(
									schema::block_proposer::dsl::cluster_address
										.eq(cluster_address),
								)
								.filter(schema::block_proposer::dsl::address.eq(address))
								.filter(schema::block_proposer::dsl::epoch.eq(epoch)),
						)
						.set(schema::block_proposer::dsl::selected_next.eq(&true)) // set new values for balance and nonce
						.execute(*conn.lock().await)
					},
					PgConnectionType::PgConn(conn) => {
						diesel::update(
							schema::block_proposer::table
								.filter(
									schema::block_proposer::dsl::cluster_address
										.eq(cluster_address),
								)
								.filter(schema::block_proposer::dsl::address.eq(address))
								.filter(schema::block_proposer::dsl::epoch.eq(epoch)),
						)
						.set(schema::block_proposer::dsl::selected_next.eq(&true)) // set new values for balance and nonce
						.execute(&mut *conn.lock().await)
					},
				}?;
			}
		}
		Ok(())
	}

	async fn store_block_proposers(
		&self,
		block_proposers: &HashMap<Address, HashMap<Epoch, Address>>,
		_selector_address: Option<Address>,
		_cluster_address: Option<Address>,
	) -> Result<(), Error> {
		for (cluster_address, epoch_numbers) in block_proposers {
			for (epoch, address) in epoch_numbers {
				self.store_block_proposer(cluster_address.clone(), *epoch, address.clone())
					.await?;
			}
		}
		/*if let Some(selector_address) = selector_address {
			if let Some(cluster_address) = cluster_address {
				self.update_selected_next(&selector_address, &cluster_address).await?;
			}
		}*/
		Ok(())
	}

	async fn load_last_block_proposer_epoch(
		&self,
		cluster_address: Address,
	) -> Result<Epoch, Error> {
		// Implementation for set_schema_version method
		let encoded_cluster_address = hex::encode(cluster_address);

		let max_epoch: BigDecimal = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::block_proposer::table
				.filter(schema::block_proposer::dsl::cluster_address.eq(encoded_cluster_address))
				.select(diesel::dsl::sql::<Numeric>("MAX(epoch)"))
				.first(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::block_proposer::table
				.filter(schema::block_proposer::dsl::cluster_address.eq(encoded_cluster_address))
				.select(diesel::dsl::sql::<Numeric>("MAX(epoch)"))
				.first(&mut *conn.lock().await),
		}?;

		let epoch_u64: u64 = match max_epoch.to_u64() {
				Some(u64_val) => u64_val,
				None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
		};

		Ok(epoch_u64)
	}

	async fn load_block_proposer(
		&self,
		cluster_addr: Address,
		_epoch: Epoch,
	) -> Result<Option<BlockProposer>, Error> {
		use db::postgres::schema::block_proposer::dsl::*;
		// Implementation for set_schema_version method
		let encoded_cluster_address = hex::encode(cluster_addr);
		let current_epoch = convert_to_big_decimal_epoch(_epoch);
		let res: QueryResult<QueryBlockProposer> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => block_proposer
				.filter(
					cluster_address.eq(&encoded_cluster_address)
						.and(epoch.eq(&current_epoch)),
				)
				.first(*conn.lock().await),
			PgConnectionType::PgConn(conn) => block_proposer
				.filter(
					cluster_address
						.eq(&encoded_cluster_address)
						.and(epoch.eq(&current_epoch)),
				)
				.first(&mut *conn.lock().await),
		};

		match res {
			Ok(results) => {
				let epoch_u64: u64 = match results.epoch.to_u64() {
					Some(u64_val) => u64_val,
					None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u64")),
				};
				let mut cluster_addr: Address = [0; 20];
				let mut bp_address: Address = [0; 20];

				let cluster = hex::decode(&results.cluster_address.clone())?;
				if cluster.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&cluster);
					cluster_addr = array;
				}

				let addr = hex::decode(&results.address.clone())?;
				if addr.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&addr);
					bp_address = array;
				}
				let block_proposer_data = BlockProposer {
					cluster_address: cluster_addr,
					address: bp_address,
					epoch: epoch_u64,
				};
				Ok(Some(block_proposer_data))
			},
			Err(e) => {
				return Err(anyhow::anyhow!("Diesel query failed: {}", e))
			},
		}
	}

	async fn load_block_proposers(
		&self,
		cluster_addr: &Address,
	) -> Result<HashMap<Epoch, BlockProposer>, Error> {
		use db::postgres::schema::block_proposer::dsl::*;
		// Implementation for set_schema_version method
		let mut block_proposers = HashMap::new();

		let encoded_cluster_address = hex::encode(cluster_addr);

		let res: QueryResult<Vec<QueryBlockProposer>> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => block_proposer
				.filter(cluster_address.eq(encoded_cluster_address))
				.load(*conn.lock().await),
			PgConnectionType::PgConn(conn) => block_proposer
				.filter(cluster_address.eq(encoded_cluster_address))
				.load(&mut *conn.lock().await),
		};

		match res {
			Ok(results) => {
				for query_result in results {
					let epoch_u64: u64 = match query_result.epoch.to_u64() {
						Some(u64_val) => u64_val,
						None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u64")),
					};

					let mut cluster_addr: Address = [0; 20];
					let mut addrss: Address = [0; 20];

					let cluster = hex::decode(&query_result.cluster_address.clone())?;
					if cluster.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&cluster);
						cluster_addr = array;
					}

					let addr = hex::decode(&query_result.address.clone())?;
					if addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&addr);
						addrss = array;
					}

					block_proposers.insert(
						epoch_u64,
						BlockProposer {
							cluster_address: cluster_addr,
							epoch: epoch_u64,
							address: addrss,
						},
					);
				}
				Ok(block_proposers)
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn find_cluster_address(&self, address: Address) -> Result<Address, Error> {
		let encoded_address = hex::encode(address);
		let result: QueryResult<String> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::block_proposer::table
				.select(schema::block_proposer::dsl::cluster_address)
				.filter(schema::block_proposer::dsl::address.eq(&encoded_address))
				.get_result::<String>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::block_proposer::table
				.select(schema::block_proposer::dsl::cluster_address)
				.filter(schema::block_proposer::dsl::address.eq(&encoded_address))
				.get_result::<String>(&mut *conn.lock().await),
		};

		match result {
			Ok(res) => {
				let mut cluster_addr: Address = [0; 20];
				let cluster = hex::decode(&res)?;
				if cluster.len() == 20 {
					cluster_addr.copy_from_slice(&cluster);
				}
				Ok(cluster_addr)
			},
			Err(e) => Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}
	async fn is_selected_next(&self, address: &Address) -> Result<bool, Error> {
		let encoded_address = hex::encode(address);
		let count: i64 = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::block_proposer::table
				.filter(
					schema::block_proposer::dsl::address
						.eq(encoded_address)
						.and(schema::block_proposer::dsl::selected_next.eq(false)),
				)
				.count()
				.get_result(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::block_proposer::table
				.filter(
					schema::block_proposer::dsl::address
						.eq(encoded_address)
						.and(schema::block_proposer::dsl::selected_next.eq(false)),
				)
				.count()
				.get_result(&mut *conn.lock().await),
		}?;

		Ok(!(count > 0))
	}

	async fn create_or_update(
		&self,
		_cluster_address: Address,
		_epoch: Epoch,
		_address: Address,
	) -> Result<(), Error> {
		use db::postgres::schema::block_proposer::dsl::*;

		// Check if an entry with the same epoch exists
		let existing_proposer: Option<QueryBlockProposer> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => block_proposer
				.filter(epoch.eq(convert_to_big_decimal_epoch(_epoch)))
				.first(*conn.lock().await)
				.optional()
				.map_or(None, |res| res) ,
			PgConnectionType::PgConn(conn) => block_proposer
				.filter(epoch.eq(convert_to_big_decimal_epoch(_epoch)))
				.first(&mut *conn.lock().await)
				.optional()
				.map_or(None, |res| res) ,
		};

		let blk_proposer = BlockProposer {
			address: _address,
			cluster_address: _cluster_address,
			epoch: _epoch,
		};

		if let Some(_) = existing_proposer {
			// Update the existing row
			let update_result = self.update(&blk_proposer).await;
			update_result.map_err(|e| anyhow::anyhow!("Failed to update block proposer: {:?}", e))?;
		} else {
			// Insert a new row
			let insert_result = self.create(&blk_proposer).await;
			insert_result.map_err(|e| anyhow::anyhow!("Failed to create block proposer: {:?}", e))?;
		}

		Ok(())
	}
}
