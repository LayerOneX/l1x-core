use anyhow::Error;
use async_trait::async_trait;
use primitives::*;
use system::validator::Validator;

#[async_trait]
pub trait ValidatorState {
	async fn load_validator(&self, address: &Address) -> Result<Validator, Error>;

	async fn load_all_validators(
		&self,
		epoch: Epoch,
	) -> Result<Option<Vec<Validator>>, Error>;

	async fn is_validator(
		&self,
		address: &Address,
		epoch: Epoch,
	) -> Result<bool, Error>;

	async fn batch_store_validators(&self, validators: &Vec<Validator>) -> Result<(), Error>;

	async fn create_or_update(
		&self,
		validator: &Validator
	) -> Result<(), Error>;

}
