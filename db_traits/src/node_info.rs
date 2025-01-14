use anyhow::Error;
use async_trait::async_trait;
use primitives::*;
use std::collections::HashMap;
use system::node_info::NodeInfo;

#[async_trait]
pub trait NodeInfoState {
	async fn store_nodes(
		&self,
		clusters: &HashMap<Address, HashMap<Address, NodeInfo>>,
	) -> Result<(), Error>;
	async fn find_node_info(
		&self,
		address: &Address,
	) -> Result<(Option<Address>, Option<NodeInfo>), Error>;
	async fn find_node_info_by_node_id(&self, node_id: Vec<u8>) -> Result<NodeInfo, Error>;
	async fn load_node_info(&self, address: &Address) -> Result<NodeInfo, Error>;
	async fn load_nodes(
		&self,
		cluster_address: &Address,
	) -> Result<HashMap<Address, NodeInfo>, Error>;
	async fn remove_node_info(&self, address: &Address) -> Result<(), Error>;
	async fn has_address_exists(&self, address: &Address) -> Result<bool, Error>;
    async fn create_or_update(&self, node_info: &NodeInfo) -> Result<(), Error>;
}