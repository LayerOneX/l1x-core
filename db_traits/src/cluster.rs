use anyhow::Error;
use async_trait::async_trait;
use primitives::*;

#[async_trait]
pub trait ClusterState {
	async fn store_cluster_address(&self, cluster_address: &Address) -> Result<(), Error>;

	async fn store_cluster_addresses(&self, cluster_addresses: &Vec<Address>) -> Result<(), Error>;

	async fn load_all_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error>;
}
