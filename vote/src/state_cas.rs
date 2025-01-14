use anyhow::{anyhow, Context, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, vote::VoteState};
use primitives::*;
use scylla::Session;
use std::{collections::HashMap, sync::Arc};
use system::vote::{Vote, VoteSignPayload};

pub struct StateCas {
	pub(crate) session: Arc<Session>,
}
#[async_trait]
impl BaseState<Vote> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		self.session
			.query(
				"CREATE TABLE IF NOT EXISTS vote (
                    block_number Bigint,
                    block_hash blob,
                    cluster_address blob,
                    validator_address blob,
                    signature blob,
                    verifying_key blob,
                    vote boolean,
                    PRIMARY KEY (block_hash, validator_address)
                );",
				&[],
			)
			.await
			.with_context(|| "Failed to create contract table")?;

		Ok(())
	}

	async fn create(&self, vote: &Vote) -> Result<(), Error> {
		let block_number: i64 = i64::try_from(vote.data.block_number).unwrap_or(i64::MAX);
		self
			.session
			.query(
				"INSERT INTO vote (block_number, block_hash, cluster_address, validator_address, signature, verifying_key, vote) VALUES (?,?,?,?,?,?,?);",
				(&block_number, &vote.data.block_hash, &vote.data.cluster_address, &vote.validator_address, &vote.signature, &vote.verifying_key, &vote.data.vote),
			)
			.await
			.with_context(|| "Failed to store vote data")?;
		Ok(())
	}

	async fn update(&self, _vote: &Vote) -> Result<(), Error> {
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
impl VoteState for StateCas {
	async fn load_all_votes(&self, block_hash: &BlockHash) -> Result<Option<Vec<Vote>>, Error> {
		let rows = self
			.session
			.query("SELECT * FROM vote WHERE block_hash = ? ;", (block_hash,))
			.await?
			.rows()?;

		let mut votes = vec![];
		for row in rows {
			let (
				block_number,
				block_hash,
				cluster_address,
				validator_address,
				signature,
				verifying_key,
				vote,
				epoch,
			) = row.into_typed::<(i64, BlockHash, Address, Address, Vec<u8>, Vec<u8>, bool, i64)>()?;

			votes.push(Vote {
				data: VoteSignPayload {
					block_number: block_number.try_into().unwrap_or(BlockNumber::MIN),
					block_hash,
					cluster_address,
					vote,
					epoch: epoch.try_into().unwrap_or(Epoch::MIN),
				},
				validator_address,
				signature,
				verifying_key,
			});
		}
		if votes.is_empty() {
			Ok(None)
		} else {
			Ok(Some(votes))
		}
	}

	async fn load_all_votes_hashmap(
		&self,
		block_hash: &BlockHash,
	) -> Result<Option<HashMap<Address, bool>>, Error> {
		let rows = self
			.session
			.query("SELECT validator_address, vote FROM vote WHERE block_hash = ? ;", (block_hash,))
			.await?
			.rows()?;

		let mut votes = HashMap::new();
		for r in rows {
			// let (_block_hash, _cluster_address, validator_address, vote) =
			let (validator_address, vote) = r.into_typed::<(Address, bool)>()?;
			votes.insert(validator_address, vote);
		}
		if votes.is_empty() {
			Ok(None)
		} else {
			Ok(Some(votes))
		}
	}
}
