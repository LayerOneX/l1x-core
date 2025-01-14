use anyhow::Error;
use async_trait::async_trait;
use primitives::*;
use std::collections::HashMap;
use system::block_proposer::BlockProposer;

#[async_trait]
pub trait BlockProposerState {
	async fn store_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
		address: Address,
	) -> Result<(), Error>;

	async fn load_selectors_block_epochs(
		&self,
		address: &Address,
	) -> Result<Option<Vec<Epoch>>, Error>;
	async fn update_selected_next(
		&self,
		selector_address: &Address,
		cluster_address: &Address,
	) -> Result<(), Error>;

	async fn store_block_proposers(
		&self,
		block_proposers: &HashMap<Address, HashMap<Epoch, Address>>,
		selector_address: Option<Address>,
		cluster_address: Option<Address>,
	) -> Result<(), Error>;

	async fn load_last_block_proposer_epoch(
		&self,
		cluster_address: Address,
	) -> Result<Epoch, Error>;

	async fn load_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
	) -> Result<Option<BlockProposer>, Error>;

	async fn load_block_proposers(
		&self,
		cluster_address: &Address,
	) -> Result<HashMap<Epoch, BlockProposer>, Error>;

	async fn find_cluster_address(&self, address: Address) -> Result<Address, Error>;
	async fn is_selected_next(&self, address: &Address) -> Result<bool, Error>;

	async fn create_or_update(
		&self,
		_cluster_address: Address,
		_epoch: Epoch,
		_address: Address,
	) -> Result<(), Error>;
}
