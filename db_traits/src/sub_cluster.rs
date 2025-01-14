use anyhow::Error;
use async_trait::async_trait;
use primitives::*;

#[async_trait]
pub trait SubClusterState {
	async fn store_sub_cluster_address(&self, sub_cluster_address: &Address) -> Result<(), Error>;

	async fn store_sub_cluster_addresses(
		&self,
		sub_cluster_addresses: &Vec<Address>,
	) -> Result<(), Error>;

	async fn load_all_sub_cluster_addresses(&self) -> Result<Option<Vec<Address>>, Error>;
}
