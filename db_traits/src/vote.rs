use anyhow::Error;
use async_trait::async_trait;
use primitives::*;
use std::collections::HashMap;
use system::vote::Vote;

#[async_trait]
pub trait VoteState {
	async fn load_all_votes(&self, block_hash: &BlockHash) -> Result<Option<Vec<Vote>>, Error>;

	async fn load_all_votes_hashmap(
		&self,
		block_hash: &BlockHash,
	) -> Result<Option<HashMap<Address, bool>>, Error>;
}
