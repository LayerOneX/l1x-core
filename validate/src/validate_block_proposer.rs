use anyhow::Error;
use system::block_proposer::BlockProposerPayload;

pub struct ValidateBlockProposer;

impl ValidateBlockProposer {
	pub async fn validate_block_proposer(
		block_proposer_payload: &BlockProposerPayload,
	) -> Result<(), Error> {
		// Verify the signature of the block header to know definitively which node sent it
		block_proposer_payload.verify_signature().await
	}
}
