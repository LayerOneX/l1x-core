use anyhow::{anyhow, Context, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, vote_result::VoteResultState};
use primitives::*;
use scylla::Session;
use std::sync::Arc;
use system::vote_result::{VoteResult, /*VoteResultSignPayload*/};

pub struct StateCas {
	pub(crate) session: Arc<Session>,
}
#[async_trait]
impl BaseState<VoteResult> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		self.session
			.query(
				"CREATE TABLE IF NOT EXISTS vote_result (
                    block_number Bigint,
                    block_hash blob,
                    cluster_address blob,
                    validator_address blob,
                    signature blob,
                    verifying_key blob,
                    vote_passed boolean,
                    PRIMARY KEY (block_hash, validator_address)
                );",
				&[],
			)
			.await
			.with_context(|| "Failed to create contract table")?;

		Ok(())
	}

	async fn create(&self, vote_result: &VoteResult) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
		// let block_number: i64 = i64::try_from(vote_result.data.block_number).unwrap_or(i64::MAX);
		// self
		// 	.session
		// 	.query(
		// 		"INSERT INTO vote_result (block_number, block_hash, cluster_address, validator_address, signature, verifying_key, vote_passed) VALUES (?,?,?,?,?,?,?);",
		// 		(&block_number, &vote_result.data.block_hash, &vote_result.data.cluster_address, &vote_result.validator_address, &vote_result.signature, &vote_result.verifying_key, &vote_result.data.vote_passed),
		// 	)
		// 	.await
		// 	.with_context(|| "Failed to store vote_passed data")?;
		// Ok(())
	}

	async fn update(&self, _u: &VoteResult) -> Result<(), Error> {
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
		todo!()
	}
}

#[async_trait]
impl VoteResultState for StateCas {
	async fn load_vote_result(&self, block_hash: &BlockHash) -> Result<VoteResult, Error> {
		Err(anyhow!("Not supported"))
		// let (
		// 	block_number,
		// 	block_hash,
		// 	cluster_address,
		// 	validator_address,
		// 	signature,
		// 	verifying_key,
		// 	vote_passed,
		// ) = self
		// 	.session
		// 	.query("SELECT block_number,block_hash,cluster_address,validator_address,signature,verifying_key,vote_passed  FROM vote_result WHERE block_hash = ? ;", (block_hash,))
		// 	.await?
		// 	.single_row()?
		// 	.into_typed::<(i64, BlockHash, Address, Address, Vec<u8>, Vec<u8>, bool)>()?;

		// Ok(VoteResult {
		// 	data: VoteResultSignPayload {
		// 		block_number: block_number.try_into().unwrap_or(BlockNumber::MIN),
		// 		block_hash,
		// 		cluster_address,
		// 		vote_passed,
		// 	},
		// 	validator_address,
		// 	signature,
		// 	verifying_key,
		// })
	}

	async fn batch_store_vote_results(&self, _vote_results: &Vec<VoteResult>) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}
}
