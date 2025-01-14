use anyhow::Error;
use async_trait::async_trait;
use bigdecimal::{BigDecimal, ToPrimitive};
use db::postgres::pg_models::{
	NewStakingAccount, NewStakingPool, QueryStakingAccount, QueryStakingPool,
};

use db::postgres::{
	postgres::{PgConnectionType, PostgresDBConn},
	schema,
};
use db_traits::{base::BaseState, staking::StakingState};
use diesel::{self, prelude::*, QueryResult};
use primitives::{Address, Balance, BlockNumber};

use system::{staking_account::StakingAccount, staking_pool::StakingPool};
use util::convert::{convert_to_big_decimal_balance, convert_to_big_decimal_block_number};
pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}
#[async_trait]
impl<'a> BaseState<StakingPool> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, staking_pool: &StakingPool) -> Result<(), Error> {
		let contract_addr = match staking_pool.contract_instance_address {
			Some(address) => hex::encode(address),
			None => "".to_string(),
		};

		let min_pool_bal = match staking_pool.min_pool_balance {
			Some(bal) => convert_to_big_decimal_balance(bal),
			None => BigDecimal::from(0),
		};

		let max_pool_bal = match staking_pool.max_pool_balance {
			Some(bal) => convert_to_big_decimal_balance(bal),
			None => BigDecimal::from(0),
		};

		let minimum_stake = match staking_pool.min_stake {
			Some(stake) => convert_to_big_decimal_balance(stake),
			None => BigDecimal::from(0),
		};
		let maximum_stake = match staking_pool.max_stake {
			Some(stake) => convert_to_big_decimal_balance(stake),
			None => BigDecimal::from(0),
		};

		let staking_period = match staking_pool.staking_period {
			Some(stake_period) => convert_to_big_decimal_balance(stake_period).to_string(),
			None => "".to_string(),
		};

		let new_staking_pool = NewStakingPool {
			pool_address: hex::encode(staking_pool.pool_address),
			cluster_address: Some(hex::encode(staking_pool.cluster_address)),
			contract_instance_address: Some(contract_addr),
			created_block_number: Some(convert_to_big_decimal_block_number(
				staking_pool.created_block_number,
			)),
			max_pool_balance: Some(max_pool_bal),
			max_stake: Some(maximum_stake),
			min_pool_balance: Some(min_pool_bal),
			min_stake: Some(minimum_stake),
			pool_owner: Some(hex::encode(staking_pool.pool_owner)),
			staking_period: Some(staking_period),
			updated_block_number: Some(convert_to_big_decimal_block_number(
				staking_pool.updated_block_number,
			)),
		};

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(schema::staking_pool::table)
				.values(new_staking_pool)
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(schema::staking_pool::table)
				.values(new_staking_pool)
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn update(&self, _staking_pool: &StakingPool) -> Result<(), Error> {
		// Implementation for update method
		todo!()
	}

	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
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
impl<'a> StakingState for StatePg<'a> {
	async fn is_staking_account_exists(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<bool, Error> {
		let encode_address = hex::encode(account_address);
		let encode_pool_address = hex::encode(pool_address);

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let res = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::staking_account::table
				.filter(
					schema::staking_account::dsl::account_address
						.eq(encode_address)
						.and(schema::staking_account::dsl::pool_address.eq(encode_pool_address)),
				)
				.load::<QueryStakingAccount>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::staking_account::table
				.filter(
					schema::staking_account::dsl::account_address
						.eq(encode_address)
						.and(schema::staking_account::dsl::pool_address.eq(encode_pool_address)),
				)
				.load::<QueryStakingAccount>(&mut *conn.lock().await),
		}?;

		// let res = schema::staking_account::table
		// 	.filter(
		// 		schema::staking_account::dsl::account_address
		// 			.eq(encode_address)
		// 			.and(schema::staking_account::dsl::pool_address.eq(encode_pool_address)),
		// 	)
		// 	.load::<QueryStakingAccount>(*self.pg.conn.lock().await)?;

		Ok(!res.is_empty())
	}
	async fn is_staking_pool_exists(&self, pool_address: &Address) -> Result<bool, Error> {
		let encode_pool_address = hex::encode(pool_address);
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		let res = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::staking_pool::table
				.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address))
				.load::<QueryStakingPool>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::staking_pool::table
				.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address))
				.load::<QueryStakingPool>(&mut *conn.lock().await),
		}?;

		// let res = schema::staking_pool::table
		// 	.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address))
		// 	.load::<QueryStakingPool>(*self.pg.conn.lock().await)?;

		Ok(!res.is_empty())
	}

	async fn update_staking_pool_block_number(
		&self,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error> {
		let encode_pool_address = hex::encode(pool_address);
		let block_num = convert_to_big_decimal_block_number(block_number);
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::update(
				schema::staking_pool::table
					.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address)),
			)
			.set(schema::staking_pool::dsl::updated_block_number.eq(block_num))
			.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::update(
				schema::staking_pool::table
					.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address)),
			)
			.set(schema::staking_pool::dsl::updated_block_number.eq(block_num))
			.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn update_contract(
		&self,
		contract_instance_address: &Address,
		pool_address: &Address,
		block_number: BlockNumber,
	) -> Result<(), Error> {
		let encode_contract_instance_address = hex::encode(contract_instance_address);
		let encode_pool_address = hex::encode(pool_address);
		let block_num = convert_to_big_decimal_block_number(block_number);
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::update(schema::staking_pool::table)
				.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address))
				.set((
					schema::staking_pool::dsl::updated_block_number.eq(block_num),
					schema::staking_pool::dsl::contract_instance_address
						.eq(encode_contract_instance_address),
				))
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::update(schema::staking_pool::table)
				.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address))
				.set((
					schema::staking_pool::dsl::updated_block_number.eq(block_num),
					schema::staking_pool::dsl::contract_instance_address
						.eq(encode_contract_instance_address),
				))
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn get_staking_pool(&self, pool_address: &Address) -> Result<StakingPool, Error> {
		let encode_pool_address = hex::encode(pool_address);
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		let res: QueryResult<QueryStakingPool> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::staking_pool::table
				.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address))
				.first(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::staking_pool::table
				.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address))
				.first(&mut *conn.lock().await),
		};

		// let res: QueryResult<QueryStakingPool> = schema::staking_pool::table
		// 	.filter(schema::staking_pool::dsl::pool_address.eq(encode_pool_address))
		// 	.first(*self.pg.conn.lock().await);

		match res {
			Ok(query_results) => {
				let mut pool_owner_addr: Address = [0; 20];
				let mut pool_addr: Address = [0; 20];
				let mut cluster_addr: Address = [0; 20];

				let acc = match query_results.pool_owner {
					Some(addr) => hex::decode(addr)?,
					None => return Err(anyhow::anyhow!("pool_owner not found")),
				};

				if acc.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&acc);
					pool_owner_addr = array;
				}

				let pool = hex::decode(query_results.pool_address.clone())?;

				if pool.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&pool);
					pool_addr = array;
				}

				let cluster = match query_results.cluster_address {
					Some(addr) => hex::decode(addr)?,
					None => return Err(anyhow::anyhow!("cluster_address not found")),
				};

				if cluster.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&cluster);
					cluster_addr = array;
				}

				let staking_pool = StakingPool {
					pool_address: pool_addr,
					pool_owner: pool_owner_addr,
					contract_instance_address: None,
					cluster_address: cluster_addr,
					created_block_number: query_results.created_block_number.map_or(0, |v| v.to_u128().unwrap_or_default()),
					updated_block_number: query_results.updated_block_number.map_or(0, |v| v.to_u128().unwrap_or_default()),
					min_stake: query_results.min_stake.map(|v| v.to_u128().unwrap_or_default()),
					max_stake: query_results.max_stake.map(|v| v.to_u128().unwrap_or_default()),
					min_pool_balance: query_results.min_pool_balance.map(|v| v.to_u128().unwrap_or_default()),
					max_pool_balance: query_results.max_pool_balance.map(|v| v.to_u128().unwrap_or_default()),
					staking_period: query_results.staking_period.map(|v| u128::from_str_radix(&v, 10).unwrap_or_default()),
				};
				Ok(staking_pool)
			},
			Err(e) => Err(anyhow::anyhow!("Failed to get staking_pool: {}", e)),
		}
	}

	async fn insert_staking_account(
		&self,
		account_address: &Address,
		pool_address: &Address,
		balance: Balance,
	) -> Result<(), Error> {
		let bal = convert_to_big_decimal_balance(balance);
		let encode_address = hex::encode(account_address);
		let encode_pool_address = hex::encode(pool_address);

		let new_staking_account = NewStakingAccount {
			pool_address: encode_pool_address,
			account_address: Some(encode_address),
			balance: Some(bal),
		};
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(schema::staking_account::table)
				.values(new_staking_account)
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(schema::staking_account::table)
				.values(new_staking_account)
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn update_staking_account(&self, staking_account: &StakingAccount) -> Result<(), Error> {
		let bal = convert_to_big_decimal_balance(staking_account.balance);
		let encode_address = hex::encode(staking_account.account_address);
		let encode_pool_address = hex::encode(staking_account.pool_address);
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;
		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::update(schema::staking_account::table)
				.filter(
					schema::staking_account::dsl::pool_address
						.eq(encode_pool_address)
						.and(schema::staking_account::dsl::account_address.eq(encode_address)),
				)
				.set(schema::staking_account::dsl::balance.eq(bal))
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::update(schema::staking_account::table)
				.filter(
					schema::staking_account::dsl::pool_address
						.eq(encode_pool_address)
						.and(schema::staking_account::dsl::account_address.eq(encode_address)),
				)
				.set(schema::staking_account::dsl::balance.eq(bal))
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn get_staking_account(
		&self,
		account_address: &Address,
		pool_address: &Address,
	) -> Result<StakingAccount, Error> {
		let encode_address = hex::encode(account_address);
		let encode_pool_address = hex::encode(pool_address);
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let res: QueryResult<QueryStakingAccount> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::staking_account::table
				.filter(
					schema::staking_account::dsl::account_address
						.eq(encode_address)
						.and(schema::staking_account::dsl::pool_address.eq(encode_pool_address)),
				)
				.first(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::staking_account::table
				.filter(
					schema::staking_account::dsl::account_address
						.eq(encode_address)
						.and(schema::staking_account::dsl::pool_address.eq(encode_pool_address)),
				)
				.first(&mut *conn.lock().await),
		};

		// let res: QueryResult<QueryStakingAccount> = schema::staking_account::table
		// 	.filter(
		// 		schema::staking_account::dsl::account_address
		// 			.eq(encode_address)
		// 			.and(schema::staking_account::dsl::pool_address.eq(encode_pool_address)),
		// 	)
		// 	.first(*self.pg.conn.lock().await);
		//

		match res {
			Ok(query_results) => {
				let mut account_addr: Address = [0; 20];
				let mut pool_addr: Address = [0; 20];

				let acc = hex::decode(query_results.account_address.clone())?;

				if acc.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&acc);
					account_addr = array;
				}

				let pool = hex::decode(query_results.pool_address.clone())?;

				if pool.len() == 20 {
					let mut array = [0u8; 20];
					array.copy_from_slice(&pool);
					pool_addr = array;
				}

				let balance_number_u128: u128 = match query_results.balance {
					Some(decimal) => match decimal.to_u128() {
						Some(u128_val) => u128_val,
						None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
					},
					None => return Err(anyhow::anyhow!("Block number is None")),
				};

				let staking_info = StakingAccount {
					account_address: account_addr,
					pool_address: pool_addr,
					balance: balance_number_u128,
				};
				Ok(staking_info)
			},
			Err(e) => Err(anyhow::anyhow!("Failed to insert new node info: {}", e)),
		}
	}

	async fn get_all_pool_stakers(
		&self,
		pool_address: &Address,
	) -> Result<Vec<StakingAccount>, Error> {
		let encode_pool_address = hex::encode(pool_address);
		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let res: QueryResult<Vec<QueryStakingAccount>> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => schema::staking_account::table
				.filter(schema::staking_account::dsl::pool_address.eq(encode_pool_address))
				.load(*conn.lock().await),
			PgConnectionType::PgConn(conn) => schema::staking_account::table
				.filter(schema::staking_account::dsl::pool_address.eq(encode_pool_address))
				.load(&mut *conn.lock().await),
		};

		let mut pool_stakers_list = vec![];
		match res {
			Ok(results) => {
				for query_results in results {
					let mut account_addr: Address = [0; 20];
					let mut pool_addr: Address = [0; 20];

					let acc = hex::decode(query_results.account_address.clone())?;

					if acc.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&acc);
						account_addr = array;
					}

					let pool = hex::decode(query_results.pool_address.clone())?;

					if pool.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&pool);
						pool_addr = array;
					}

					let balance_number_u128: u128 = match query_results.balance {
						Some(decimal) => match decimal.to_u128() {
							Some(u128_val) => u128_val,
							None =>
								return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
						},
						None => return Err(anyhow::anyhow!("Block number is None")),
					};

					let staking_info = StakingAccount {
						account_address: account_addr,
						pool_address: pool_addr,
						balance: balance_number_u128,
					};
					pool_stakers_list.push(staking_info)
				}
				Ok(pool_stakers_list)
			},
			Err(e) => Err(anyhow::anyhow!("Failed to get all pool stakers: {}", e)),
		}
	}
}
