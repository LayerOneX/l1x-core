use std::time::Duration;
use anyhow::Error;
use async_trait::async_trait;
use bigdecimal::{BigDecimal, ToPrimitive};
use db::postgres::{
	pg_models::{NewValidator, QueryValidator},
	postgres::{PgConnectionType, PostgresDBConn},
	schema,
};
use db_traits::{base::BaseState, validator::ValidatorState};
use diesel::{self, prelude::*};
use tokio::time::{Instant, sleep};
use primitives::{Address, BlockNumber, Epoch};
use system::validator::Validator;
use util::convert::{convert_f64_to_big_decimal, convert_to_big_decimal_block_number, convert_to_big_decimal_epoch};

pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}

#[async_trait]
impl<'a> BaseState<Validator> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, _validator: &Validator) -> Result<(), Error> {
		let new_epoch: BigDecimal = convert_to_big_decimal_epoch(_validator.epoch);
		let _stake: BigDecimal = convert_to_big_decimal_block_number(_validator.stake);
		let xscore_num: BigDecimal = convert_f64_to_big_decimal(_validator.xscore);

		let new_validator = NewValidator {
			address: hex::encode(_validator.address),
			epoch: new_epoch,
			cluster_address: Some(hex::encode(_validator.cluster_address)),
			stake: Some(_stake),
			xscore: Some(xscore_num),
		};

		use db::postgres::schema::validator::dsl::*;

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(validator)
				.values(new_validator)
				.on_conflict((address, epoch))
				.do_nothing()
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(validator)
				.values(new_validator)
				.on_conflict((address, epoch))
				.do_nothing()
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())

	}

	async fn update(&self, _validator: &Validator) -> Result<(), Error> {
		let new_epoch: BigDecimal = convert_to_big_decimal_epoch(_validator.epoch);
		let _stake: BigDecimal = convert_to_big_decimal_block_number(_validator.stake);
		let xscore_num: BigDecimal = convert_f64_to_big_decimal(_validator.xscore);

		use db::postgres::schema::validator::dsl::*;

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) =>
				diesel::update(validator
					.filter(address.eq(hex::encode(_validator.address)))
					.filter(epoch.eq(convert_to_big_decimal_epoch(_validator.epoch))))
					.set((
						address.eq(hex::encode(_validator.address)),
						epoch.eq(new_epoch),
						cluster_address.eq(hex::encode(_validator.cluster_address)),
						stake.eq(_stake),
						xscore.eq(xscore_num),
					))
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) =>
				diesel::update(validator
					.filter(address.eq(hex::encode(_validator.address)))
					.filter(epoch.eq(convert_to_big_decimal_epoch(_validator.epoch))))
					.set((
							 address.eq(hex::encode(_validator.address)),
							 epoch.eq(new_epoch),
							 cluster_address.eq(hex::encode(_validator.cluster_address)),
							 stake.eq(_stake),
							 xscore.eq(xscore_num),
						 ))
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
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
		todo!()
	}
}

#[async_trait]
impl<'a> ValidatorState for StatePg<'a> {
	async fn load_validator(&self, _address: &Address) -> Result<Validator, Error> {
		let encoded_address = hex::encode(_address);

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		let res: Result<QueryValidator, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::validator::table
				.filter(schema::validator::dsl::address.eq(encoded_address.clone()))
				.first::<QueryValidator>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::validator::table
				.filter(schema::validator::dsl::address.eq(encoded_address.clone()))
				.first::<QueryValidator>(&mut *conn.lock().await),
		};

		// let res: Result<QueryValidator, diesel::result::Error> = schema::validator::table
		// 	.filter(schema::validator::dsl::address.eq(encoded_address.clone()))
		// 	.first(*self.pg.conn.lock().await);

		match res {
			Ok(query_results) => {
				let mut cluster_add: Address = [0; 20];
				let mut address: Address = [0; 20];

				let cluster = hex::decode(
					query_results.cluster_address.clone().unwrap_or_else(|| "".to_string()),
				)?;

				if cluster.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&cluster);
					cluster_add = array;
				}

				let addr = hex::decode(query_results.address.clone())?;

				if addr.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&addr);
					address = array;
				}

				let epoch_u64: u64 = match query_results.epoch.to_u64() {
					Some(u64_val) => u64_val,
					None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u64")),
				};

				let stake_u128: u128 = match query_results.stake.unwrap().to_u128() {
					Some(u128_val) => u128_val,
					None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
				};

				let xscore_f64: f64 = match query_results.xscore.to_f64() {
					Some(f64_val) => f64_val,
					None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to f64")),
				};

				let data = Validator {
					address,
					epoch: epoch_u64,
					stake: stake_u128,
					cluster_address: cluster_add,
					xscore: xscore_f64,
				};

				Ok(data)
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}
	
	async fn load_all_validators(
		&self,
		_epoch: Epoch,
	) -> Result<Option<Vec<Validator>>, Error> {
		use db::postgres::schema::validator::dsl::*;
		let epoch_u64: BigDecimal = convert_to_big_decimal_epoch(_epoch);
		let mut validator_list: Vec<Validator> = vec![];

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let res: Result<Vec<QueryValidator>, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => validator
				.filter(epoch.eq(epoch_u64.clone()))
				.load::<QueryValidator>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => validator
				.filter(epoch.eq(epoch_u64.clone()))
				.load::<QueryValidator>(&mut *conn.lock().await),
		};

		match res {
			Ok(results) => {
				for query_results in results {
					let mut cluster_add: Address = [0; 20];
					let mut validator_address: Address = [0; 20];

					let cluster = hex::decode(
						query_results.cluster_address.clone().unwrap_or_else(|| "".to_string()),
					)?;

					if cluster.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&cluster);
						cluster_add = array;
					}

					let addr = hex::decode(query_results.address.clone())?;

					if addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&addr);
						validator_address = array;
					}

					let epoch_u64: u64 = match query_results.epoch.to_u64() {
						Some(u64_val) => u64_val,
						None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u64")),
					};

					let stake_u128: u128 = match query_results.stake.unwrap().to_u128() {
						Some(u128_val) => u128_val,
						None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
					};

					let xscore_f64: f64 = match query_results.xscore.to_f64() {
						Some(f64_val) => f64_val,
						None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to f64")),
					};

					let data = Validator {
						address: validator_address,
						epoch: epoch_u64,
						stake: stake_u128,
						cluster_address: cluster_add,
						xscore: xscore_f64,
					};

					validator_list.push(data)
				}
				if !validator_list.is_empty() {
					Ok(Some(validator_list))
				} else {
					Ok(None)
				}

			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn is_validator(
		&self,
		address: &Address,
		_epoch: Epoch,
	) -> Result<bool, Error> {
		let epoch_u64: BigDecimal = convert_to_big_decimal_epoch(_epoch);
		let encoded_address = hex::encode(address);
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let res = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::validator::table
				.filter(
					schema::validator::dsl::address
						.eq(encoded_address)
						.and(schema::validator::dsl::epoch.eq(epoch_u64)),
				)
				.load::<QueryValidator>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::validator::table
				.filter(
					schema::validator::dsl::address
						.eq(encoded_address)
						.and(schema::validator::dsl::epoch.eq(epoch_u64)),
				)
				.load::<QueryValidator>(&mut *conn.lock().await),
		}?;

		Ok(!res.is_empty())
	}

	async fn batch_store_validators(&self, validators: &Vec<Validator>) -> Result<(), Error> {
		let mut block_validators: Vec<NewValidator> = vec![];
		for _validator in validators {
			let epoch_u64: BigDecimal = convert_to_big_decimal_epoch(_validator.epoch);
			let _stake: BigDecimal = convert_to_big_decimal_block_number(_validator.stake);
			let xscore_num: BigDecimal = convert_f64_to_big_decimal(_validator.xscore);

			let new_validator = NewValidator {
				address: hex::encode(_validator.address),
				epoch: epoch_u64,
				cluster_address: Some(hex::encode(_validator.cluster_address)),
				stake: Some(_stake),
				xscore: Some(xscore_num),
			};
			block_validators.push(new_validator);
		}

		use db::postgres::schema::validator::dsl::*;

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(validator)
				.values(block_validators)
				.on_conflict((address, epoch))
				.do_nothing()
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(validator)
				.values(block_validators)
				.on_conflict((address, epoch))
				.do_nothing()
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn create_or_update(
		&self,
		_validator: &Validator
	) -> Result<(), Error> {
		use db::postgres::schema::validator::dsl::*;

		// Check if an entry with the same epoch exists
		let existing_validator: Option<QueryValidator> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => validator
				.filter(address.eq(hex::encode(_validator.address)))
				.filter(epoch.eq(convert_to_big_decimal_epoch(_validator.epoch)))
				.first(*conn.lock().await)
				.optional()
				.map_or(None, |res| res) ,
			PgConnectionType::PgConn(conn) => validator
				.filter(address.eq(hex::encode(_validator.address)))
				.filter(epoch.eq(convert_to_big_decimal_epoch(_validator.epoch)))
				.first(&mut *conn.lock().await)
				.optional()
				.map_or(None, |res| res),
		};

		if let Some(_) = existing_validator {
			// Update the existing row
			let update_result = self.update(_validator).await;
			update_result.map_err(|e| anyhow::anyhow!("Failed to update validator: {:?}", e))?;
		} else {
			// Insert a new row
			let insert_result = self.create(_validator).await;
			insert_result.map_err(|e| anyhow::anyhow!("Failed to store validator: {:?}", e))?;
		}

		Ok(())
	}
}
