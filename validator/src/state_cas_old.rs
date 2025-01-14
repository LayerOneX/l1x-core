use anyhow::{anyhow, Context, Error};
use db::{
    cassandra::DatabaseManager,
    utils::{FromByteArray, ToByteArray},
};
use log::error;
use primitives::{arithmetic::ScalarBig, *};
use scylla::Session;
use std::sync::Arc;
use system::validator::Validator;
pub struct ValidatorState {
    pub session: Arc<Session>,
}

impl ValidatorState {
    pub async fn new() -> Result<Self, Error> {
        let db_session = DatabaseManager::get_session().await?;
        let validator_state = ValidatorState { session: db_session.clone() };
        validator_state.create_table().await?;
        Ok(validator_state)
    }

    pub async fn get_state() -> Result<Self, Error> {
        let db_session = DatabaseManager::get_session().await?;
        let state = ValidatorState { session: db_session.clone() };
        state.create_table().await?;
        Ok(state)
    }

    pub async fn create_table(&self) -> Result<(), Error> {
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

    pub async fn store_validator(&self, validator: &Validator) -> Result<(), Error> {
        let block_number: i64 = i64::try_from(validator.block_number).unwrap_or(i64::MAX);
        let stake_bytes: ScalarBig = validator.stake.to_byte_array_le(ScalarBig::default());

        self
            .session
            .query(
                "INSERT INTO validator (address, cluster_address, block_number, stake) VALUES (?, ?, ?, ?);",
                (&validator.address, &validator.cluster_address, &block_number, &stake_bytes),
            )
            .await
            .with_context(|| "Failed to store validator data")?;
        Ok(())
    }

    pub async fn load_validator(&self, address: &Address) -> Result<Validator, Error> {
        let result = self
            .session
            .query("SELECT * FROM validator WHERE address = ? ALLOW FILTERING;", (&address,))
            .await;

        let (validator_address, block_number, cluster_address, stake_bytes) = match result {
            Ok(query_result) => {
                let p = query_result.single_row()?;
                let q = match p.into_typed::<(Address, i64, Address, Vec<u8>)>() {
                    Ok(q) => q,
                    Err(err) => {
                        // Log the error
                        let message = format!("Error loading validator: {}", err);
                        error!("{}", message);
                        return Err(anyhow!("{}", message))
                    },
                };
                q
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

        let block_number = block_number.try_into().unwrap_or(BlockNumber::MIN);
        let stake = u128::from_byte_array(&stake_bytes);

        Ok(Validator { address: validator_address, cluster_address, block_number, stake })
    }

    // pub async fn load_validator(&self, address: &Address) -> Result<Validator, Error> {
    // 	let (validator_address, cluster_address, block_number_bytes, block_hash, stake_bytes) =
    // 		self.session
    // 			.query(
    // 				"SELECT * FROM validator WHERE address = ? ALLOW FILTERING;",
    // 				(&address,),
    // 			)
    // 			.await?
    // 			.single_row()?
    // 			.into_typed::<(Address, Address, Vec<u8>, BlockHash, Vec<u8>)>()?;
    //
    // 	let block_number = u128::from_byte_array(&block_number_bytes);
    // 	let stake = u128::from_byte_array(&stake_bytes);
    //
    // 	Ok(Validator {
    // 		address: validator_address,
    // 		cluster_address,
    // 		block_number,
    // 		block_hash,
    // 		stake,
    // 	})
    // }

    pub async fn load_all_validators(
        &self,
        block_number: BlockNumber,
    ) -> Result<Option<Vec<Validator>>, Error> {
        let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
        let rows = self
            .session
            .query(
                "SELECT * FROM validator WHERE block_number = ? ALLOW FILTERING;",
                (&block_number,),
            )
            .await?
            .rows()?;

        let mut validators = Vec::new();
        for row in rows {
            let (validator_address, block_number, cluster_address, stake_bytes) =
                row.into_typed::<(Address, i64, Address, Vec<u8>)>()?;
            let block_number = block_number.try_into().unwrap_or(BlockNumber::MIN);
            let stake = u128::from_byte_array(&stake_bytes);
            validators.push(Validator {
                address: validator_address,
                cluster_address,
                block_number,
                stake,
            });
        }
        if validators.is_empty() {
            Ok(None)
        } else {
            Ok(Some(validators))
        }
    }

    pub async fn is_validator(
        &self,
        address: &Address,
        block_number: BlockNumber,
    ) -> Result<bool, Error> {
        let block_number: i64 = i64::try_from(block_number).unwrap_or(i64::MAX);
        let (count,) = self
            .session
            .query(
                "SELECT COUNT(*) AS count FROM validator WHERE address = ? AND block_number = ? ALLOW FILTERING ;",
                (address, block_number,),
            )
            .await?
            .single_row()?
            .into_typed::<(i64,)>()?;

        Ok(count > 0)
    }
}