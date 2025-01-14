use anyhow::{Error, Result};
use primitives::*;
use system::vote::{Vote, VoteSignPayload};
pub struct VoteManager;

impl<'a> VoteManager {
	pub async fn vote(
		block_number: BlockNumber,
		block_hash: BlockHash,
		cluster_address: Address,
		validator_address: Address,
		validator_signature: SignatureBytes,
		validator_verifying_key: VerifyingKeyBytes,
		vote: bool,
		epoch: Epoch,
	) -> Result<Vote, Error> {
		Ok(Vote::new(
			VoteSignPayload::new(block_number, block_hash, cluster_address, epoch, vote),
			validator_address,
			validator_signature,
			validator_verifying_key,
		))
	}
}
