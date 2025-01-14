use anyhow::Error;
use async_trait::async_trait;
use db_traits::{base::BaseState, sub_cluster::SubClusterState};
use primitives::Address;
use rocksdb::DB;
use std::fs::remove_dir_all;
pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DB,
}

#[async_trait]
impl BaseState<Address> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		// Implementation for create_table method
		Ok(())
	}

	async fn create(&self, _address: &Address) -> Result<(), Error> {
		// Implementation for create method
		todo!()
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
impl SubClusterState for StateRock {
	async fn store_sub_cluster_address(&self, _cluster_address: &Address) -> Result<(), Error> {
		Ok(())
	}

	async fn store_sub_cluster_addresses(
		&self,
		_cluster_addresses: &Vec<Address>,
	) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}

	async fn load_all_sub_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error> {
		// Implementation for set_schema_version method
		todo!()
	}
}
