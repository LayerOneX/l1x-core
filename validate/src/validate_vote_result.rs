use anyhow::Error;
use db::db::DbTxConn;
use system::{block::Block, vote::Vote, vote_result::VoteResult};
use anyhow::anyhow;
use log::debug;
use compile_time_config::voting_config::VOTE_THRESHOLD;
use crate::validate_vote::ValidateVote;

pub struct ValidateVoteResult;

impl ValidateVoteResult {
	pub async fn validate_vote_result<'a>(vote_result: &VoteResult, block: &Block, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		let block_number = block.block_header.block_number;

		debug!("validate_vote_result ~ Received Block number: {}, Vote result block number: {}", block.block_header.block_number, vote_result.data.block_number);
		debug!("validate_vote_result ~ Received Block hash: {}, Vote result block hash: {}", hex::encode(block.block_header.block_hash), hex::encode(vote_result.data.block_hash));
		debug!("validate_vote_result ~ Received Cluster address: {}, Vote result cluster address: {}", hex::encode(block.block_header.cluster_address), hex::encode(vote_result.data.cluster_address));
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

		let mut valid_votes = 0;
		let mut total_votes = 0;

		for vote in &vote_result.data.votes {
			total_votes += 1;
			
			// Validate individual vote
			match ValidateVoteResult::validate_single_vote(vote, vote_result, block, db_pool_conn).await {
				Ok(_) => {
					valid_votes += 1;
				},
				Err(e) => {
					// Log the error but continue processing other votes
					log::warn!("Invalid vote from validator {:?}: {}", 
						hex::encode(&vote.validator_address), e);
					continue;
				}
			}
		}

		let min_required_votes = ((total_votes as f64 * VOTE_THRESHOLD) - f64::EPSILON).ceil() as u32;

		log::info!("validate_vote_result ~ Valid votes: {}, Total votes: {}, Min required votes: {}", valid_votes, total_votes, min_required_votes);
		// Check if we have enough valid votes to consider the vote result valid
		if valid_votes < min_required_votes {
			return Err(anyhow!(
				"VoteResult: Insufficient valid votes. Valid: {}, Total: {}, Block #{}",
				valid_votes,
				total_votes,
				block_number
			));
		}

		Ok(())
	}

	async fn validate_single_vote<'a>(
		vote: &Vote, 
		vote_result: &VoteResult,
		block: &Block,
		db_pool_conn: &'a DbTxConn<'a>
	) -> Result<(), Error> {
		if vote.data.block_number != vote_result.data.block_number {
			return Err(anyhow!(
				"Incorrect Vote block number: actual {}, expected: {}",
				vote.data.block_number,
				vote_result.data.block_number
			));
		}
		if vote.data.block_hash != vote_result.data.block_hash {
			return Err(anyhow!(
				"Incorrect Vote block hash: actual {}, expected: {}",
				hex::encode(&vote.data.block_hash),
				hex::encode(&vote_result.data.block_hash),
			));
		}
		if vote.data.cluster_address != vote_result.data.cluster_address {
			return Err(anyhow!(
				"Incorrect Vote cluster address: actual {}, expected: {}",
				hex::encode(&vote.data.cluster_address),
				hex::encode(&vote_result.data.cluster_address),
			));
		}
		ValidateVote::validate_vote(vote, block, db_pool_conn).await
	}
}
