use anyhow::{anyhow, Error};
use async_trait::async_trait;

use db_traits::{base::BaseState, block_proposer::BlockProposerState};

use primitives::*;

use scylla::{Session, _macro_internal::CqlValue};
use std::{collections::HashMap, sync::Arc};
use system::block_proposer::BlockProposer;

pub struct StateCas {
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<BlockProposer> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS block_proposer (
					cluster_address blob,
					epoch Bigint,
					address blob,
					selected_next boolean,
					PRIMARY KEY (address, cluster_address, epoch)
				);",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Create table failed: {}", e)),
		};
		Ok(())
	}

	async fn create(&self, _block_proposer: &BlockProposer) -> Result<(), Error> {
		todo!()
	}

	async fn update(&self, _block_proposer: &BlockProposer) -> Result<(), Error> {
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
impl BlockProposerState for StateCas {
	async fn store_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
		address: Address,
	) -> Result<(), Error> {
		let epoch: i64 = i64::try_from(epoch).unwrap_or(i64::MAX);
		match self
			.session
			.query(
				"INSERT INTO block_proposer (cluster_address, epoch, address, selected_next) VALUES (?, ?, ?, ?);",
				(&cluster_address, &epoch, &address, &false),
			)
			.await
		{
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed to store block proposer - {}", e)),
		};
		Ok(())
	}

	async fn load_selectors_block_epochs(
		&self,
		address: &Address,
	) -> Result<Option<Vec<Epoch>>, Error> {
		let rows = self
			.session
			.query(
				"SELECT address, epoch FROM block_proposer WHERE address = ? allow filtering;",
				(address,),
			)
			.await?
			.rows()?;

		let mut epoch_numbers = Vec::new();
		for r in rows {
			let (_address, epoch) = r.into_typed::<(Address, i64)>()?;
			epoch_numbers.push(epoch.try_into().unwrap_or(Epoch::MIN));
		}
		if epoch_numbers.is_empty() {
			Ok(None)
		} else {
			Ok(Some(epoch_numbers))
		}
	}

	async fn update_selected_next(
		&self,
		selector_address: &Address,
		cluster_address: &Address,
	) -> Result<(), Error> {
		if let Some(epoch_numbers) = self.load_selectors_block_epochs(selector_address).await? {
			for epoch in epoch_numbers {
				let epoch: i64 = i64::try_from(epoch).unwrap_or(i64::MAX);
				match self
					.session
					.query(
						"UPDATE block_proposer SET selected_next = ? WHERE address = ? and cluster_address = ? and epoch = ?;",
						(&true, selector_address, cluster_address, &epoch),
					)
					.await
				{
					Ok(_) => {},
					Err(e) => return Err(anyhow!("Failed to update block proposer - {}", e)),
				}
			}
		}
		Ok(())
	}
	async fn store_block_proposers(
		&self,
		block_proposers: &HashMap<Address, HashMap<Epoch, Address>>,
		selector_address: Option<Address>,
		cluster_address: Option<Address>,
	) -> Result<(), Error> {
		for (cluster_address, epoch_numbers) in block_proposers {
			for (epoch, address) in epoch_numbers {
				self.store_block_proposer(cluster_address.clone(), *epoch, address.clone())
					.await?;
			}
		}
		if let Some(selector_address) = selector_address {
			if let Some(cluster_address) = cluster_address {
				self.update_selected_next(&selector_address, &cluster_address).await?;
			}
		}
		Ok(())
	}

	async fn load_last_block_proposer_epoch(
		&self,
		cluster_address: Address,
	) -> Result<Epoch, Error> {
		let (epoch,)= self
			.session
			.query(
				"SELECT MAX(epoch) FROM block_proposer WHERE cluster_address = ? allow filtering;",
				(&cluster_address,),
			)
			.await?
			.single_row()?
			.into_typed::<(i64,)>()?;

		let epoch = epoch.try_into().unwrap_or(Epoch::MIN);

		Ok(epoch)
	}

	async fn load_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
	) -> Result<Option<BlockProposer>, Error> {
		let epoch1: i64 = i64::try_from(epoch).unwrap_or(i64::MAX);
		let query_result = match self
			.session
			.query(
				"SELECT address FROM block_proposer WHERE cluster_address = ? AND epoch = ? allow filtering;",
				(&cluster_address, &epoch1),
			)
			.await
		{
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Failed to load block proposer - {}", e)),
		};

		let (address_idx, _) = query_result
			.get_column_spec("address")
			.ok_or_else(|| anyhow!("No address column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		let address: Address = if let Some(row) = rows.get(0) {
			if let Some(address_value) = &row.columns[address_idx] {
				if let CqlValue::Blob(address) = address_value {
					address.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
				} else {
					return Err(anyhow!("Unable to convert to Address type"))
				}
			} else {
				return Err(anyhow!("Unable to read epoch column"))
			}
		} else {
			return Err(anyhow!("block_proposer_state : address 167: Unable to read row"))
		};

		Ok(Some(BlockProposer { cluster_address, address, epoch }))
	}

	async fn load_block_proposers(
		&self,
		cluster_address: &Address,
	) -> Result<HashMap<Epoch, BlockProposer>, Error> {
		let rows = self
			.session
			.query("SELECT cluster_address, epoch, address FROM block_proposer WHERE cluster_address = ? allow filtering;", (cluster_address,))
			.await?
			.rows()?;

		let mut block_proposers = HashMap::new();
		for row in rows {
			let (cluster_address, epoch, address) =
				row.into_typed::<(Address, i64, Address)>()?;
			let epoch = epoch.try_into().unwrap_or(Epoch::MIN);
			block_proposers
				.insert(epoch, BlockProposer { cluster_address, address, epoch });
		}
		Ok(block_proposers)
	}

	async fn find_cluster_address(&self, address: Address) -> Result<Address, Error> {
		let query_result = match self
			.session
			.query(
				"SELECT cluster_address FROM block_proposer WHERE address = ? ALLOW FILTERING;",
				(&address,),
			)
			.await
		{
			Ok(q) => q,
			Err(e) => return Err(anyhow!("Failed to find full node info - {}", e)),
		};

		let (cluster_address_idx, _) = query_result
			.get_column_spec("cluster_address")
			.ok_or_else(|| anyhow!("No cluster_address column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		if let Some(row) = rows.get(0) {
			let cluster_address = if let Some(cluster_address_value) =
				&row.columns[cluster_address_idx]
			{
				if let CqlValue::Blob(cluster_address) = cluster_address_value {
					cluster_address.as_slice().try_into().map_err(|_| anyhow!("Invalid length"))?
				} else {
					return Err(anyhow!("Unable to convert to Address type"))
				}
			} else {
				return Err(anyhow!("Unable to read cluster_address column"))
			};
			Ok(cluster_address)
		} else {
			return Err(anyhow!("block_proposer_state:  cluster_address 276 : Unable to read row"))
		}
	}

	async fn is_selected_next(&self, address: &Address) -> Result<bool, Error> {
		let (count,) = self
			.session
			.query("SELECT COUNT(*) AS count FROM block_proposer WHERE address = ? AND selected_next = false allow filtering;", (address,))
			.await?
			.single_row()?
			.into_typed::<(i64,)>()?;

		Ok(!(count > 0))
	}

	async fn create_or_update(&self, _cluster_address: Address, _epoch: Epoch, _address: Address) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}
}
