use anyhow::{Context, Error};
use db::{
    cassandra::DatabaseManager,
    utils::{FromByteArray, ToByteArray},
};
use primitives::{arithmetic::ScalarBig, *};
use scylla::Session;
use std::sync::Arc;
use system::vote_result::VoteResult;
use vote::vote_state::VoteState;

pub struct VoteResultState {
    pub session: Arc<Session>,
}

impl VoteResultState {
    pub async fn new() -> Result<Self, Error> {
        let db_session = DatabaseManager::get_session().await?;
        let vote_state = VoteResultState { session: db_session.clone() };
        vote_state.create_table().await?;
        Ok(vote_state)
    }

    pub async fn create_table(&self) -> Result<(), Error> {
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

    pub async fn store_vote_result(&self, vote_result: &VoteResult) -> Result<(), Error> {
        let block_number: i64 = i64::try_from(vote_result.block_number).unwrap_or(i64::MAX);
        self
            .session
            .query(
                "INSERT INTO vote_result (block_number, block_hash, cluster_address, validator_address, signature, verifying_key, vote_passed) VALUES (?,?,?,?,?,?,?);",
                (&block_number, &vote_result.block_hash, &vote_result.cluster_address, &vote_result.validator_address, &vote_result.signature, &vote_result.verifying_key, &vote_result.vote_passed),
            )
            .await
            .with_context(|| "Failed to store vote_passed data")?;
        Ok(())
    }

    pub async fn load_vote_result(&self, block_hash: &BlockHash) -> Result<VoteResult, Error> {
        let (
            block_number,
            block_hash,
            cluster_address,
            validator_address,
            signature,
            verifying_key,
            vote_passed,
        ) = self
            .session
            .query("SELECT block_number,block_hash,cluster_address,validator_address,signature,verifying_key,vote_passed  FROM vote_result WHERE block_hash = ? ;", (block_hash,))
            .await?
            .single_row()?
            .into_typed::<(i64, BlockHash, Address, Address, Vec<u8>, Vec<u8>, bool)>()?;

        Ok(VoteResult {
            block_number: block_number.try_into().unwrap_or(BlockNumber::MIN),
            block_hash,
            cluster_address,
            validator_address,
            signature,
            verifying_key,
            vote_passed,
        })
    }
}
