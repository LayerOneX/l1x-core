use anyhow::Error;
use async_trait::async_trait;
use primitives::Epoch;
use system::node_health::NodeHealth;

#[async_trait]
pub trait NodeHealthState {
	async fn store_node_health(&self, node_health: &NodeHealth) -> Result<(), Error>;
	async fn load_node_health(&self, peer_id: &str, epoch: Epoch) -> Result<Option<NodeHealth>, Error>;
	async fn update_node_health(&self, node_health: &NodeHealth) -> Result<(), Error>;
	async fn load_node_healths(&self, epoch: Epoch) -> Result<Vec<NodeHealth>, Error>;
	async fn create_or_update(&self, _node_health: &NodeHealth) -> Result<(), Error>;
}