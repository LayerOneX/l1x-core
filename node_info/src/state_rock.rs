use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, node_info::NodeInfoState};
use primitives::Address;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use std::{collections::HashMap, fs::remove_dir_all};
use system::node_info::NodeInfo;

pub struct StateRock {
	pub(crate) db_path: String,
	pub db: DBWithThreadMode<MultiThreaded>,
}
#[async_trait]
impl BaseState<NodeInfo> for StateRock {
	async fn create_table(&self) -> Result<(), Error> {
		Ok(())
	}

	async fn create(&self, _node_info: &NodeInfo) -> Result<(), Error> {
		// Implementation for create method
		Ok(())
	}

	async fn update(&self, _node_info: &NodeInfo) -> Result<(), Error> {
		// Implementation for update method
		Ok(())
	}

	async fn raw_query(&self, _query: &str) -> Result<(), Error> {
		// Remove the database directory
		remove_dir_all(&self.db_path)?;

		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		// Implementation for set_schema_version method
		Ok(())
	}
}

#[async_trait]
impl NodeInfoState for StateRock {
	async fn store_nodes(
		&self,
		_clusters: &HashMap<Address, HashMap<Address, NodeInfo>>,
	) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}

	async fn find_node_info(
		&self,
		_address: &Address,
	) -> Result<(Option<Address>, Option<NodeInfo>), Error> {
		Ok((None, None))
	}
	
	async fn find_node_info_by_node_id(&self, _node_id: Vec<u8>) -> Result<NodeInfo, Error> {
		todo!()
	}

	async fn load_node_info(&self, _address: &Address) -> Result<NodeInfo, Error> {
		todo!()
	}

	async fn load_nodes(
		&self,
		_cluster_address: &Address,
	) -> Result<HashMap<Address, NodeInfo>, Error> {
		todo!()
	}

	async fn remove_node_info(&self, _address: &Address) -> Result<(), Error> {
		// Implementation for set_schema_version method
		todo!()
	}
	async fn has_address_exists(&self, _address: &Address) -> Result<bool, Error> {
		todo!()
	}

	async fn create_or_update(&self, node_info: &NodeInfo) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}
}
