use anyhow::{anyhow, Error};
use async_trait::async_trait;
use primitives::{Address, BlockNumber, Epoch};
use rocksdb::DB;
use std::fs::remove_dir_all;
use system::validator::Validator;

use db_traits::{base::BaseState, validator::ValidatorState};

pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}

#[async_trait]
impl BaseState<Validator> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, _validator: &Validator) -> Result<(), Error> {
		todo!()
	}

	async fn update(&self, _validator: &Validator) -> Result<(), Error> {
		todo!()
	}

	async fn raw_query(&self, _query: &str) -> Result<(), Error> {
		// Remove the database directory
		remove_dir_all(&self.db_path)?;

		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		todo!()
	}
}

#[async_trait]
impl ValidatorState for StateRock {
	async fn load_validator(&self, _address: &Address) -> Result<Validator, Error> {
		todo!()
	}

	async fn load_all_validators(
		&self,
		_epoch: Epoch,
	) -> Result<Option<Vec<Validator>>, Error> {
		todo!()
	}

	async fn is_validator(
		&self,
		_address: &Address,
		_epoch: Epoch,
	) -> Result<bool, Error> {
		todo!()
	}

	async fn batch_store_validators(&self, _validators: &Vec<Validator>) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}

	async fn create_or_update(&self, _validators: &Validator) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}
}