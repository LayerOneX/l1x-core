use anyhow::Error;
use system::node_info::NodeInfo;

pub struct ValidateNodeInfo;

impl ValidateNodeInfo {
	pub async fn validate_node_info(node_info: &NodeInfo) -> Result<(), Error> {
		// Verify the signature of the node info to know definitively which node sent it
		node_info.verify_signature().await
	}
}
