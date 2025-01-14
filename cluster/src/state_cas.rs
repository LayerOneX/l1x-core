use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, cluster::ClusterState};
use primitives::*;
use scylla::{Session, _macro_internal::CqlValue};
use std::sync::Arc;

pub struct StateCas {
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<Address> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS cluster (
                    cluster_address blob,
                    PRIMARY KEY (cluster_address)
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

	async fn create(&self, _u: &Address) -> Result<(), Error> {
		todo!()
	}

	async fn update(&self, _u: &Address) -> Result<(), Error> {
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
impl ClusterState for StateCas {
	async fn store_cluster_address(&self, cluster_address: &Address) -> Result<(), Error> {
		match self
			.session
			.query("INSERT INTO cluster (cluster_address) VALUES (?);", (&cluster_address,))
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Failed to store cluster address - {}", e)),
		}
	}

	async fn store_cluster_addresses(&self, cluster_addresses: &Vec<Address>) -> Result<(), Error> {
		for cluster_address in cluster_addresses {
			self.store_cluster_address(cluster_address).await?;
		}
		Ok(())
	}

	async fn load_all_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error> {
		let query_result =
			match self.session.query("SELECT cluster_address FROM cluster;", &[]).await {
				Ok(q) => q,
				Err(e) => return Err(anyhow!("Failed to load cluster IDs - {}", e)),
			};

		let (cluster_address_idx, _) = query_result
			.get_column_spec("cluster_address")
			.ok_or_else(|| anyhow!("No cluster_address column found"))?;

		let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

		let mut cluster_addresses = Vec::new();

		for row in rows {
			if let Some(cluster_address_value) = &row.columns[cluster_address_idx] {
				if let CqlValue::Blob(cluster_address) = cluster_address_value {
					let cluster_address = cluster_address
						.as_slice()
						.try_into()
						.map_err(|_| anyhow!("Invalid length"))?;
					cluster_addresses.push(cluster_address);
				} else {
					return Err(anyhow!("Unable to convert to Address type"))
				}
			}
		}

		if cluster_addresses.is_empty() {
			Ok(None)
		} else {
			Ok(Some(cluster_addresses))
		}
	}
}
