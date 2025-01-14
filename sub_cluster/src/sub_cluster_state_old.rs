use anyhow::{anyhow, Error};
use db::cassandra::DatabaseManager;
use primitives::*;
use scylla::{Session, _macro_internal::CqlValue};
use std::sync::Arc;

pub struct SubClusterState {
	pub session: Arc<Session>,
}

impl SubClusterState {
	pub async fn new() -> Result<Self, Error> {
		let db_session = DatabaseManager::get_session().await?;
		let sub_cluster_state = SubClusterState { session: db_session.clone() };
		sub_cluster_state.create_table().await?;
		Ok(sub_cluster_state)
	}

	pub async fn create_table(&self) -> Result<(), Error> {
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS sub_cluster (
                    sub_cluster_address blob,
                    PRIMARY KEY (sub_cluster_address)
                );",
				&[],
			)
			.await
		{
			Ok(_) => {},
			Err(_) => return Err(anyhow!("Create table failed")),
		};
		Ok(())
	}

	pub async fn store_sub_cluster_address(
		&self,
		sub_cluster_address: &Address,
	) -> Result<(), Error> {
		match self
			.session
			.query(
				"INSERT INTO sub_cluster (sub_cluster_address) VALUES (?);",
				(&sub_cluster_address,),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Failed to store Sub cluster ID - {}", e)),
		}
	}

	pub async fn store_sub_cluster_addresses(
		&self,
		sub_cluster_addresses: &Vec<Address>,
	) -> Result<(), Error> {
		for sub_cluster_address in sub_cluster_addresses {
			self.store_sub_cluster_address(sub_cluster_address).await?;
		}
		Ok(())
	}

	pub async fn load_all_sub_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error> {
		let query_result =
			match self.session.query("SELECT sub_cluster_address FROM sub_cluster;", &[]).await {
				Ok(q) => q,
				Err(_) => return Err(anyhow!("Failed to load cluster IDs")),
			};

		let (sub_cluster_address_idx, _) = query_result
			.get_column_spec("sub_cluster_address")
			.ok_or_else(|| anyhow!("No sub_cluster_address column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		let mut sub_cluster_addresses = Vec::new();

		for row in rows {
			if let Some(sub_cluster_address_value) = &row.columns[sub_cluster_address_idx] {
				if let CqlValue::Blob(sub_cluster_address) = sub_cluster_address_value {
					let sub_cluster_address = sub_cluster_address
						.as_slice()
						.try_into()
						.map_err(|_| anyhow!("Invalid length"))?;
					sub_cluster_addresses.push(sub_cluster_address);
				} else {
					return Err(anyhow!("Unable to convert to Address type"))
				}
			}
		}

		if sub_cluster_addresses.is_empty() {
			Ok(None)
		} else {
			Ok(Some(sub_cluster_addresses))
		}
	}
}
