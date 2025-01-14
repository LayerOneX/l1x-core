use anyhow::Error;
use db::db::DbTxConn;
use system::{block::Block, vote_result::VoteResult};
use anyhow::anyhow;

use crate::validate_vote::ValidateVote;

pub struct ValidateVoteResult;

impl ValidateVoteResult {
	pub async fn validate_vote_result<'a>(vote_result: &VoteResult, block: &Block, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		let block_number = block.block_header.block_number;

		if block.block_header.block_number != vote_result.data.block_number {
			return Err(anyhow!(
				"VoteResult: Incorrect block number: actual {}, expected: {}",
				vote_result.data.block_number,
				block.block_header.block_number
			));
		}
		if block.block_header.block_hash != vote_result.data.block_hash {
			return Err(anyhow!(
				"VoteResult: Incorrect block hash: actual {}, expected: {}, block #{}",
				hex::encode(vote_result.data.block_hash),
				hex::encode(block.block_header.block_hash),
				block_number
			));
		}
		if block.block_header.cluster_address != vote_result.data.cluster_address {
			return Err(anyhow!(
				"VoteResult: Incorrect cluster address: actual {}, expected: {}, block #{}",
				hex::encode(vote_result.data.cluster_address),
				hex::encode(block.block_header.cluster_address),
				block_number
			));
		}

		vote_result.verify_signature().await?;

		for vote in &vote_result.data.votes {
			if vote.data.block_number != vote_result.data.block_number {
				return Err(anyhow!(
					"VoteResult: Incorrect Vote block number: actual {}, expected: {}",
					vote.data.block_number,
					vote_result.data.block_number
				));
			}
			if vote.data.block_hash != vote_result.data.block_hash {
				return Err(anyhow!(
					"VoteResult: Incorrect Vote block hash: actual {}, expected: {}, block #{}",
					hex::encode(&vote.data.block_hash),
					hex::encode(&vote_result.data.block_hash),
					vote_result.data.block_number,
				));
			}
			if vote.data.cluster_address != vote_result.data.cluster_address {
				return Err(anyhow!(
					"VoteResult: Incorrect Vote cluster address: actual {}, expected: {}, block #{}",
					hex::encode(&vote.data.cluster_address),
					hex::encode(&vote_result.data.cluster_address),
					vote_result.data.block_number
				));
			}
			ValidateVote::validate_vote(vote, block, db_pool_conn).await?;
		}
		Ok(())
	}
}
