use anyhow::Error;
use system::validator::ValidatorPayload;

pub struct ValidateValidatorPayload;

impl ValidateValidatorPayload {
	pub async fn validate_validator_payload(
		validator_payload: &ValidatorPayload,
	) -> Result<(), Error> {
		// Verify the signature of the validator payload
		validator_payload.verify_signature().await
	}
}
