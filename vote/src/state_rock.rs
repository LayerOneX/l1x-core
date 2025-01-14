use anyhow::Error;
use async_trait::async_trait;
use db_traits::{base::BaseState, vote::VoteState};
use primitives::{Address, BlockHash};
use rocksdb::DB;

use std::{collections::HashMap, fs::remove_dir_all};
use system::vote::Vote;
pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}

#[async_trait]
impl BaseState<Vote> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, _vote: &Vote) -> Result<(), Error> {
		todo!()
	}

	async fn update(&self, _vote: &Vote) -> Result<(), Error> {
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
impl VoteState for StateRock {
	async fn load_all_votes(&self, _block_hash: &BlockHash) -> Result<Option<Vec<Vote>>, Error> {
		todo!()
	}

	async fn load_all_votes_hashmap(
		&self,
		_block_hash: &BlockHash,
	) -> Result<Option<HashMap<Address, bool>>, Error> {
		todo!()
	}
}
