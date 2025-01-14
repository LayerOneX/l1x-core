use anyhow::Error;
use async_trait::async_trait;
use db_traits::{base::BaseState, vote::VoteState};
use primitives::{Address, BlockHash};

use bigdecimal::{BigDecimal, ToPrimitive};
use db::postgres::{
	pg_models::{NewVote, QueryVote},
	postgres::{PgConnectionType, PostgresDBConn},
};
use diesel::{self, prelude::*};
use std::collections::HashMap;
use system::vote::{Vote, VoteSignPayload};
use util::convert::{convert_to_big_decimal_block_number, convert_to_big_decimal_epoch};

pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}

#[async_trait]
impl<'a> BaseState<Vote> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, _vote: &Vote) -> Result<(), Error> {
		let block_num: BigDecimal = convert_to_big_decimal_block_number(_vote.data.block_number);
		let epoch_num: BigDecimal = convert_to_big_decimal_epoch(_vote.data.epoch);
		let new_vote = NewVote {
			block_hash: hex::encode(_vote.data.block_hash),
			block_number: Some(block_num),
			cluster_address: Some(hex::encode(_vote.data.cluster_address)),
			validator_address: Some(hex::encode(_vote.validator_address)),
			signature: Some(hex::encode(_vote.signature.clone())),
			verifying_key: Some(hex::encode(_vote.verifying_key.clone())),
			voting: Some(_vote.data.vote),
			epoch: Some(epoch_num)
		};

		use db::postgres::schema::vote::dsl::*;

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(vote)
				.values(new_vote)
				.on_conflict((block_hash, validator_address))
				.do_nothing()
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(vote)
				.values(new_vote)
				.on_conflict((block_hash, validator_address))
				.do_nothing()
				.execute(&mut *conn.lock().await),
		}?;
		Ok(())
	}

	async fn update(&self, _vote: &Vote) -> Result<(), Error> {
		Ok(())
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
		todo!()
	}
}

#[async_trait]
impl<'a> VoteState for StatePg<'a> {
	async fn load_all_votes(&self, _block_hash: &BlockHash) -> Result<Option<Vec<Vote>>, Error> {
		use db::postgres::schema::vote::dsl::*;
		let mut vote_list: Vec<Vote> = vec![];
		let encoded_block_hash = hex::encode(_block_hash);

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let res: Result<Vec<QueryVote>, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => vote
				.filter(block_hash.eq(encoded_block_hash.clone()))
				.load::<QueryVote>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => vote
				.filter(block_hash.eq(encoded_block_hash.clone()))
				.load::<QueryVote>(&mut *conn.lock().await),
		};

		match res {
			Ok(results) => {
				for query_results in results {
					let mut cluster_add: Address = [0; 20];
					let mut validator_addr: Address = [0; 20];
					let mut blockhash: BlockHash = [0; 32];
					let bh = hex::decode(query_results.block_hash.clone())?;

					if bh.len() == 32 {
						let mut array = [0u8; 32];
						array.copy_from_slice(&bh);
						blockhash = array;
					}

					let addr = hex::decode(
						query_results.cluster_address.clone().unwrap_or_else(|| "".to_string()),
					)?;

					if addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&addr);
						cluster_add = array;
					}

					let v_addr = hex::decode(query_results.validator_address.clone())?;

					if v_addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&v_addr);
						validator_addr = array;
					}

					let block_number_u128: u128 = match query_results.block_number {
						Some(decimal) => match decimal.to_u128() {
							Some(u128_val) => u128_val,
							None =>
								return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
						},
						None => return Err(anyhow::anyhow!("Block number is None")),
					};

					let epoch_number_u64: u64 = match query_results.epoch {
						Some(decimal) => match decimal.to_u64() {
							Some(u64_val) => u64_val,
							None =>
								return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
						},
						None => return Err(anyhow::anyhow!("Block number is None")),
					};
					
					let sig = hex::decode(query_results.signature.clone().unwrap()).unwrap();
					let verify_key =
						hex::decode(query_results.verifying_key.clone().unwrap()).unwrap();

					let data = VoteSignPayload {
						block_number: block_number_u128,
						block_hash: blockhash,
						cluster_address: cluster_add,
						vote: query_results.voting.clone().unwrap(),
						epoch: epoch_number_u64,
					};

					let vote_res = Vote {
						data,
						validator_address: validator_addr,
						signature: sig,
						verifying_key: verify_key,
					};

					vote_list.push(vote_res)
				}

				Ok(Some(vote_list))
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn load_all_votes_hashmap(
		&self,
		_block_hash: &BlockHash,
	) -> Result<Option<HashMap<Address, bool>>, Error> {
		let mut votes = HashMap::new();
		let encoded_block_hash = hex::encode(_block_hash);
		use db::postgres::schema::vote::dsl::*;

		// let conn: &mut PooledConnection<ConnectionManager<PgConnection>> =
		// 	&mut *self.pg.conn.pool_conn.lock().await;

		let res: Result<Vec<QueryVote>, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => vote
				.filter(block_hash.eq(encoded_block_hash.clone()))
				.load::<QueryVote>(*conn.lock().await),
			PgConnectionType::PgConn(conn) => vote
				.filter(block_hash.eq(encoded_block_hash.clone()))
				.load::<QueryVote>(&mut *conn.lock().await),
		};

		match res {
			Ok(results) => {
				for query_results in results {
					let mut validator_addr: Address = [0; 20];

					let v_addr = hex::decode(query_results.validator_address.clone())?;

					if v_addr.len() == 20 {
						let mut array = [0u8; 20];
						array.copy_from_slice(&v_addr);
						validator_addr = array;
					}

					votes.insert(validator_addr, query_results.voting.unwrap());
				}
				Ok(Some(votes))
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}
}
