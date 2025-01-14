use primitives::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockMetaInfo {
	pub cluster_address: Address,
	pub block_number: BlockNumber,
	pub block_executed: bool,
}

impl BlockMetaInfo {
	pub fn new(
		cluster_address: Address,
		block_number: BlockNumber,
		block_executed: bool,
	) -> BlockMetaInfo {
		BlockMetaInfo { cluster_address, block_number, block_executed }
	}
}
