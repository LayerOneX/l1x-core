use primitives::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainState {
	pub cluster_address: Address,
	pub block_number: BlockNumber,
	pub block_hash: BlockHash,
}

impl ChainState {
	pub fn new(
		cluster_address: Address,
		block_number: BlockNumber,
		block_hash: BlockHash,
	) -> ChainState {
		ChainState { cluster_address, block_number, block_hash }
	}
}
