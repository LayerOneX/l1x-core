use anyhow::{anyhow, Context, Error};
use async_trait::async_trait;
use db::utils::{FromByteArray, ToByteArray};
use db_traits::{base::BaseState, validator::ValidatorState};
use log::error;
use primitives::{arithmetic::ScalarBig, *};
use scylla::Session;
use std::sync::Arc;
use system::validator::Validator;
pub struct StateCas {
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<Validator> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		self.session
			.query(
				"CREATE TABLE IF NOT EXISTS validator (
					address blob,
					cluster_address blob,
					block_number Bigint,
					stake blob,
					PRIMARY KEY (address, block_number)
				);",
				&[],
			)
			.await
			.with_context(|| "Failed to create contract table")?;

		Ok(())
	}

	async fn create(&self, validator: &Validator) -> Result<(), Error> {
		let epoch: i64 = i64::try_from(validator.epoch).unwrap_or(i64::MAX);
		let stake_bytes: ScalarBig = validator.stake.to_byte_array_le(ScalarBig::default());

		self
			.session
			.query(
				"INSERT INTO validator (address, cluster_address, block_number, stake) VALUES (?, ?, ?, ?);",
				(&validator.address, &validator.cluster_address, &epoch, &stake_bytes),
			)
			.await
			.with_context(|| "Failed to store validator data")?;
		Ok(())
	}

	async fn update(&self, _validator: &Validator) -> Result<(), Error> {
		todo!()
	}

	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match self.session.query(query, &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed to execute raw query: {}", e)),
		};
		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		todo!()
	}
}

#[async_trait]
impl ValidatorState for StateCas {
	async fn load_validator(&self, address: &Address) -> Result<Validator, Error> {
		let result = self
			.session
			.query("SELECT address, block_number, cluster_address, stake FROM validator WHERE address = ? ALLOW FILTERING;", (&address,))
			.await;

		let (validator_address, epoch, cluster_address, stake_bytes) = match result {
			Ok(query_result) => {
				let p = query_result.single_row()?;
				match p.into_typed::<(Address, i64, Address, Vec<u8>)>() {
					Ok(q) => q,
					Err(err) => {
						// Log the error
						let message = format!("Error loading validator: {}", err);
						error!("{}", message);
						return Err(anyhow!("{}", message))
					},
				}
				// query_result.single_row()?.into_typed::<(Address, Address, Vec<u8>,
				// BlockHash, Vec<u8>)>()?
			},
			Err(err) => {
				// Log the error
				let message = format!("Error loading validator: {}", err);
				error!("{}", message);
				return Err(anyhow!("{}", message))
			},
		};

		let epoch = epoch.try_into().unwrap_or(Epoch::MIN);
		let stake = u128::from_byte_array(&stake_bytes);

		Ok(Validator { address: validator_address, cluster_address, epoch, stake, xscore: 0.0 })
	}

	async fn load_all_validators(
		&self,
		epoch: Epoch,
	) -> Result<Option<Vec<Validator>>, Error> {
		let epoch: i64 = i64::try_from(epoch).unwrap_or(i64::MAX);
		let rows = self
			.session
			.query(
				"SELECT address, block_number, cluster_address, stake FROM validator WHERE block_number = ? ALLOW FILTERING;",
				(&epoch,),
			)
			.await?
			.rows()?;

		let mut validators = Vec::new();
		for row in rows {
			let (validator_address, block_number, cluster_address, stake_bytes) =
				row.into_typed::<(Address, i64, Address, Vec<u8>)>()?;
			let epoch = epoch.try_into().unwrap_or(Epoch::MIN);
			let stake = u128::from_byte_array(&stake_bytes);
			validators.push(Validator {
				address: validator_address,
				cluster_address,
				epoch,
				stake,
				xscore: 0.0,
			});
		}
		if validators.is_empty() {
			Ok(None)
		} else {
			Ok(Some(validators))
		}
	}

	async fn is_validator(
		&self,
		address: &Address,
		epoch: Epoch,
	) -> Result<bool, Error> {
		let epoch: i64 = i64::try_from(epoch).unwrap_or(i64::MAX);
		let (count,) = self
			.session
			.query(
				"SELECT COUNT(*) AS count FROM validator WHERE address = ? AND block_number = ? ALLOW FILTERING ;",
				(address, epoch,),
			)
			.await?
			.single_row()?
			.into_typed::<(i64,)>()?;

		Ok(count > 0)
	}

	async fn batch_store_validators(&self, _validators: &Vec<Validator>) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}

	async fn create_or_update(&self, _validators: &Validator) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}
}