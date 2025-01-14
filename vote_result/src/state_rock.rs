use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, vote_result::VoteResultState};
use primitives::BlockHash;
use rocksdb::DB;

use std::fs::remove_dir_all;
use system::vote_result::VoteResult;
pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}

#[async_trait]
impl BaseState<VoteResult> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, _vote_result: &VoteResult) -> Result<(), Error> {
		todo!()
	}

	async fn update(&self, _vote_result: &VoteResult) -> Result<(), Error> {
		todo!()
	}

	async fn raw_query(&self, _query: &str) -> Result<(), Error> {
		// Remove the database directory
		remove_dir_all(&self.db_path)?;

		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		todo!()
	}
}
#[async_trait]
impl VoteResultState for StateRock {
	async fn load_vote_result(&self, _block_hash: &BlockHash) -> Result<VoteResult, Error> {
		todo!()
	}

	async fn batch_store_vote_results(&self, _vote_results: &Vec<VoteResult>) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}
}
