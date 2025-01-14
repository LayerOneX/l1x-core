use anyhow::{anyhow, Error};

use async_trait::async_trait;
 use bigdecimal::{BigDecimal, ToPrimitive};
use db::postgres::{
	pg_models::{NewVoteResult, QueryVoteResult},
	postgres::{PgConnectionType, PostgresDBConn},
};
use db_traits::{base::BaseState, vote_result::VoteResultState};
use diesel::{self, prelude::*};
use primitives::{Address, BlockHash};
use system::vote::Vote;
use system::vote_result::{VoteResult, VoteResultSignPayload};
use util::convert::{convert_to_big_decimal_block_number, convert_to_big_decimal_epoch};

pub struct StatePg<'a> {
	pub(crate) pg: &'a PostgresDBConn<'a>,
}

#[async_trait]
impl<'a> BaseState<VoteResult> for StatePg<'a> {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, _vote_result: &VoteResult) -> Result<(), Error> {
		let block_num: BigDecimal =
			convert_to_big_decimal_block_number(_vote_result.data.block_number);
		let _epoch = _vote_result
			.data
			.votes
			.iter()
			.next()
			.map(|vote| vote.data.epoch)
			.ok_or_else(|| anyhow!("Unable to fetch epoch in vote"))?;
		let _epoch: BigDecimal = convert_to_big_decimal_epoch(_epoch);
		let serialized_votes = bincode::serialize(&_vote_result.data.votes)
			.map_err(|e| Error::msg(format!("Failed to serialize votes: {}", e)))?;
		let new_vote_result = NewVoteResult {
			block_hash: hex::encode(_vote_result.data.block_hash),
			block_number: Some(block_num),
			cluster_address: Some(hex::encode(_vote_result.data.cluster_address)),
			validator_address: Some(hex::encode(_vote_result.validator_address)),
			signature: Some(hex::encode(_vote_result.signature.clone())),
			verifying_key: Some(hex::encode(_vote_result.verifying_key.clone())),
			vote_passed: Some(_vote_result.data.vote_passed),
			epoch: Some(_epoch),
			votes: Some(serialized_votes)
		};

		use db::postgres::schema::vote_result::dsl::*;

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(vote_result)
				.values(new_vote_result)
				.on_conflict((block_hash, validator_address))
				.do_nothing()
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(vote_result)
				.values(new_vote_result)
				.on_conflict((block_hash, validator_address))
				.do_nothing()
				.execute(&mut *conn.lock().await),
		}?;

		Ok(())
	}

	async fn update(&self, _vote_result: &VoteResult) -> Result<(), Error> {
		// Implementation for update method
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
impl<'a> VoteResultState for StatePg<'a> {
	async fn load_vote_result(&self, _block_hash: &BlockHash) -> Result<VoteResult, Error> {
		// Implementation for load_vote_result method
		use db::postgres::schema::vote_result::dsl::*;
		let encoded_block_hash = hex::encode(_block_hash);

		let res: Result<QueryVoteResult, diesel::result::Error> = match &self.pg.conn {
			PgConnectionType::TxConn(conn) => vote_result
				.filter(block_hash.eq(&encoded_block_hash))
				.first(*conn.lock().await),
			PgConnectionType::PgConn(conn) => vote_result
				.filter(block_hash.eq(&encoded_block_hash))
				.first(&mut *conn.lock().await),
		};

		match res {
			Ok(query_results) => {
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
						None => return Err(anyhow::anyhow!("Failed to convert BigDecimal to u128")),
					},
					None => return Err(anyhow::anyhow!("Block number is None")),
				};

				let sig = hex::decode(query_results.signature.clone().unwrap()).unwrap();
				let verify_key = hex::decode(query_results.verifying_key.clone().unwrap()).unwrap();

				// Deserialize votes if present
				let deserialized_votes: Vec<Vote> = match query_results.votes {
					Some(votes_bytes) => bincode::deserialize(&votes_bytes)
						.map_err(|e| anyhow!("Failed to deserialize votes: {}", e))?,
					None => Vec::new(),
				};

				let vote_signed_payload = VoteResultSignPayload {
					block_number: block_number_u128,
					block_hash: blockhash,
					cluster_address: cluster_add,
					vote_passed: query_results.vote_passed.clone().unwrap(),
					votes: deserialized_votes,
				};
				let vote_res = VoteResult {
					data: vote_signed_payload,
					validator_address: validator_addr,
					signature: sig,
					verifying_key: verify_key,
				};
				Ok(vote_res)
			},
			Err(e) => return Err(anyhow::anyhow!("Diesel query failed: {}", e)),
		}
	}

	async fn batch_store_vote_results(&self, _vote_results: &Vec<VoteResult>) -> Result<(), Error> {
		use db::postgres::schema::vote_result::dsl::*;
		let mut vote_results: Vec<NewVoteResult> = vec![];
		for _vote_result in _vote_results {
			let block_num: BigDecimal =
				convert_to_big_decimal_block_number(_vote_result.data.block_number);
			let _epoch = _vote_result
				.data
				.votes
				.iter()
				.next()
				.map(|vote| vote.data.epoch)
				.ok_or_else(|| anyhow!("Unable to fetch epoch in vote"))?;
			let _epoch: BigDecimal = convert_to_big_decimal_epoch(_epoch);
			let serialized_votes = bincode::serialize(&_vote_result.data.votes)
				.map_err(|e| Error::msg(format!("Failed to serialize votes: {}", e)))?;
			let new_vote_result = NewVoteResult {
				block_hash: hex::encode(_vote_result.data.block_hash),
				block_number: Some(block_num),
				cluster_address: Some(hex::encode(_vote_result.data.cluster_address)),
				validator_address: Some(hex::encode(_vote_result.validator_address)),
				signature: Some(hex::encode(_vote_result.signature.clone())),
				verifying_key: Some(hex::encode(_vote_result.verifying_key.clone())),
				vote_passed: Some(_vote_result.data.vote_passed),
				epoch: Some(_epoch),
				votes: Some(serialized_votes)
			};

			vote_results.push(new_vote_result);
		}

		match &self.pg.conn {
			PgConnectionType::TxConn(conn) => diesel::insert_into(vote_result)
				.values(vote_results)
				.execute(*conn.lock().await),
			PgConnectionType::PgConn(conn) => diesel::insert_into(vote_result)
				.values(vote_results)
				.execute(&mut *conn.lock().await),
		}?;

		Ok(())
	}
}
