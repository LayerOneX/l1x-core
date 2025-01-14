use crate::cluster_state::ClusterState;
use anyhow::Error;
use primitives::*;

pub struct ClusterManager {}

impl<'a> ClusterManager {
	pub async fn create_cluster(
		&mut self,
		cluster_address: &Address,
		cluster_state: &ClusterState<'a>,
	) -> Result<(), Error> {
		cluster_state.store_cluster_address(cluster_address).await?;
		Ok(())
	}
}
