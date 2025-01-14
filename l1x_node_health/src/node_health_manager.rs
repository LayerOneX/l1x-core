use anyhow::Error;
use db::db::DbTxConn;
use primitives::*;
use system::node_health::NodeHealth;
use crate::node_health_state::NodeHealthState;

pub struct NodeHealthManager;

impl<'a> NodeHealthManager {
	pub async fn store_node_health(
		&self,
		node_health: &NodeHealth,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let node_health_state = NodeHealthState::new(db_pool_conn).await?;
		node_health_state.store_node_health(node_health).await?;
		Ok(())
	}

	pub async fn load_node_health(
		&self,
		peer_id: &str,
		epoch: Epoch,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<Option<NodeHealth>, Error> {
		let node_health_state = NodeHealthState::new(db_pool_conn).await?;
		node_health_state.load_node_health(peer_id, epoch).await
	}

	pub async fn update_node_health(
		&self,
		node_health: &NodeHealth,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let node_health_state = NodeHealthState::new(db_pool_conn).await?;
		node_health_state.update_node_health(node_health).await?;
		Ok(())
	}

	pub async fn calculate_health_score(
		&self,
		peer_id: &str,
		epoch: Epoch,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<f64, Error> {
		let node_health_state = NodeHealthState::new(db_pool_conn).await?;
		let node_health = node_health_state.load_node_health(peer_id, epoch).await?
			.ok_or_else(|| anyhow::anyhow!("Node health not found for peer_id: {} at epoch: {}", peer_id, epoch))?;

		// This is a simple example of health score calculation. You may want to adjust this based on your specific requirements.
		let uptime_score = node_health.uptime_percentage / 100.0;
		let response_time_score = 1.0 - (node_health.response_time_ms as f64 / 1000.0).min(1.0);
		let transaction_score = (node_health.transaction_count as f64 / 1000.0).min(1.0);
		let block_proposal_score = (node_health.block_proposal_count as f64 / 100.0).min(1.0);

		let health_score = (uptime_score + response_time_score + transaction_score + block_proposal_score) / 4.0;

		Ok(health_score)
	}
}