use anyhow::Error;
use async_trait::async_trait;
use db_traits::{base::BaseState, cluster::ClusterState};
use rocksdb::{DBWithThreadMode, MultiThreaded, WriteBatch};
use std::{fs::remove_dir_all, result::Result::Ok};
pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DBWithThreadMode<MultiThreaded>,
}

use primitives::Address;

#[async_trait]
impl BaseState<Address> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, address: &Address) -> Result<(), Error> {
		let key = format!("cluster:{:?}", address);
		let value = serde_json::to_string(address)?;
		self.db.put(key, value)?;
		Ok(())
	}

	async fn update(&self, _address: &Address) -> Result<(), Error> {
		// Implementation for update method
		todo!()
	}

	async fn raw_query(&self, _query: &str) -> Result<(), Error> {
		// Remove the database directory
		remove_dir_all(&self.db_path)?;

		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}
}

#[async_trait]
impl ClusterState for StateRock {
	async fn store_cluster_address(&self, cluster_address: &Address) -> Result<(), Error> {
		let key = "cluster_address".to_string();

		let mut batch = WriteBatch::default();
		let cluster_addresses_result = self.load_all_cluster_addresses().await;

		if let Ok(cluster_addresses) = cluster_addresses_result {
			// Check if the Option contains Some variant
			if let Some(mut cluster_vector) = cluster_addresses {
				// Insert in the begining as per
				// test_store_cluster_address_and_load_all_cluster_addresses order
				cluster_vector.insert(0, *cluster_address);
				let value = serde_json::to_string(&cluster_vector)?;
				batch.put(&key, &value);
				self.db.write(batch)?;
			} else {
				let new_cluster_vector = vec![*cluster_address];

				let value = serde_json::to_string(&new_cluster_vector)?;
				batch.put(&key, &value);
				self.db.write(batch)?;
			}
		}

		Ok(())
	}

	async fn store_cluster_addresses(&self, cluster_addresses: &Vec<Address>) -> Result<(), Error> {
		for cluster_address in cluster_addresses {
			self.store_cluster_address(cluster_address).await?;
		}
		Ok(())
	}

	async fn load_all_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error> {
		let key = "cluster_address".to_string();

		let cluster_addresses = match self.db.get(key)? {
			Some(value) => serde_json::from_slice(&value)?,
			None => Vec::new(),
		};

		if cluster_addresses.is_empty() {
			Ok(None)
		} else {
			Ok(Some(cluster_addresses))
		}
	}
}
