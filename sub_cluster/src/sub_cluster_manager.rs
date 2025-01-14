use crate::sub_cluster_state::SubClusterState;
use anyhow::Error;

use primitives::*;

pub struct SubClusterManager {}

impl<'a> SubClusterManager {
	pub async fn create_sub_cluster(
		&mut self,
		sub_cluster_address: &Address,
		sub_cluster_state: &SubClusterState<'a>,
	) -> Result<(), Error> {
		sub_cluster_state.store_sub_cluster_address(sub_cluster_address).await?;
		Ok(())
	}
}
