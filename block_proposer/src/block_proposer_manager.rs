use crate::block_proposer_state::BlockProposerState;
use anyhow::{Error};
use db::db::DbTxConn;
use primitives::{Address, BlockNumber, Epoch};
use std::collections::HashMap;
use system::block_header::BlockHeader;
use system::block_proposer::BlockProposer;
use system::validator::Validator;
use validator::validator_manager::ValidatorManager;

pub struct BlockProposerManager {}

impl<'a> BlockProposerManager {

	pub async fn select_block_proposers(
		&mut self,
		epoch: Epoch,
		block_header: &BlockHeader,
		validators: Vec<Validator>,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<Address, Error> {
		let mut eligible_validators = validators;
		let seed = ValidatorManager.calculate_seed(block_header.block_hash, epoch);
	
		let mut cluster_block_proposers = HashMap::new();
		let mut block_proposers = HashMap::new();

		let proposer_index = self.calculate_proposer_index(seed, eligible_validators.len());
		let selected_address = eligible_validators[proposer_index].address;
		block_proposers.insert(epoch, selected_address);
	
		cluster_block_proposers.insert(block_header.cluster_address, block_proposers.clone());

		let block_proposer_state = BlockProposerState::new(db_pool_conn).await?;
		block_proposer_state
			.store_block_proposers(
				&cluster_block_proposers,
				None,
				None,
			).await?;
	
		Ok(selected_address)
	}
	
	// simple helper method to help determine the index of a selected node
	fn calculate_proposer_index(&self, seed: u64, validator_count: usize) -> usize {
		(seed % validator_count as u64) as usize
	}

	pub async fn is_block_proposer(
		&self,
		block_proposer: BlockProposer,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<bool, Error> {
		let block_proposer_state = BlockProposerState::new(db_pool_conn).await?;
		let loaded_proposer =
			block_proposer_state.load_block_proposer(block_proposer.cluster_address, block_proposer.epoch).await?;
		// Compare after converting loaded_proposer to Option<BlockProposer>
		let response = loaded_proposer == Some(block_proposer.clone());
		log::debug!("Is {:?} block proposer {}", block_proposer, response);
		Ok(response)

	}

	pub async fn get_block_proposer_for_epoch(
		&mut self,
		block_number: BlockNumber,
		cluster_address: Address,
		epoch: Epoch,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<BlockProposer, Error> {
		let block_proposer_state = BlockProposerState::new(db_pool_conn).await?;
		match block_proposer_state.load_block_proposer(cluster_address, epoch).await {
			Ok(Some(proposer)) => {
				log::debug!("Loaded proposer for epoch {}: {}", epoch, hex::encode(proposer.address));
				Ok(proposer)
			},
			Ok(None) => {
				return Err(anyhow::anyhow!("No block proposer found"));
			},
			Err(e) => {
				log::debug!("No existing proposer found for epoch {}. Selecting new proposer.", epoch);
				return Err(anyhow::anyhow!("Failed to load proposer {:?}",e));
			}
		}
	}
}