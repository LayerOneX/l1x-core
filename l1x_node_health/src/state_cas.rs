use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, node_health::NodeHealthState};
use primitives::*;
use scylla::Session;
use scylla::frame::response::result::CqlValue;
use std::sync::Arc;
use system::node_health::NodeHealth;

pub struct StateCas {
    pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<NodeHealth> for StateCas {
    async fn create_table(&self) -> Result<(), Error> {
        let query = "
            CREATE TABLE IF NOT EXISTS node_health (
                measured_peer_id text,
                peer_id text,
                epoch bigint,
                joined_epoch bigint,
                uptime_percentage double,
                response_time_ms bigint,
                transaction_count bigint,
                block_proposal_count bigint,
                anomaly_score double,
                node_health_version int,
                PRIMARY KEY ((peer_id), epoch, measured_peer_id)
            ) WITH CLUSTERING ORDER BY (epoch DESC)
        ";
        
        self.session.query(query, &[]).await?;
        Ok(())
    }

    async fn create(&self, node_health: &NodeHealth) -> Result<(), Error> {
        let query = "INSERT INTO node_health (measured_peer_id, peer_id, epoch, joined_epoch, uptime_percentage, response_time_ms, transaction_count, block_proposal_count, anomaly_score, node_health_version) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        
        self.session.query(query, (
            &node_health.measured_peer_id,
            &node_health.peer_id,
            node_health.epoch as i64,
            node_health.joined_epoch as i64,
            node_health.uptime_percentage,
            node_health.response_time_ms as i64,
            node_health.transaction_count as i64,
            node_health.block_proposal_count as i64,
            node_health.anomaly_score,
            node_health.node_health_version as i32,
        )).await?;

        Ok(())
    }

    async fn update(&self, _node_health: &NodeHealth) -> Result<(), Error> {
        // Implementation for update method
        Ok(())
    }

    async fn raw_query(&self, query: &str) -> Result<(), Error> {
        self.session.query(query, &[]).await?;
        Ok(())
    }

    async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
        // Implementation for set_schema_version method
        Ok(())
    }
}

#[async_trait]
impl NodeHealthState for StateCas {
    async fn store_node_health(&self, node_health: &NodeHealth) -> Result<(), Error> {
        self.create(node_health).await
    }

    async fn load_node_health(&self, peer_id: &str, epoch: Epoch) -> Result<Option<NodeHealth>, Error> {
        let query = "SELECT * FROM node_health WHERE peer_id = ? AND epoch = ?";
        let query_result = match self.session.query(query, (peer_id, epoch as i64)).await {
            Ok(q) => q,
            Err(e) => return Err(anyhow!("Failed to load node health - {}", e)),
        };

        let (measured_peer_id_idx, _) = query_result
            .get_column_spec("measured_peer_id")
            .ok_or_else(|| anyhow!("No measured_peer_id column found"))?;
        let (peer_id_idx, _) = query_result
            .get_column_spec("peer_id")
            .ok_or_else(|| anyhow!("No peer_id column found"))?;
        let (epoch_idx, _) = query_result
            .get_column_spec("epoch")
            .ok_or_else(|| anyhow!("No epoch column found"))?;
        let (joined_epoch_idx, _) = query_result
            .get_column_spec("joined_epoch")
            .ok_or_else(|| anyhow!("No joined_epoch column found"))?;
        let (uptime_percentage_idx, _) = query_result
            .get_column_spec("uptime_percentage")
            .ok_or_else(|| anyhow!("No uptime_percentage column found"))?;
        let (response_time_ms_idx, _) = query_result
            .get_column_spec("response_time_ms")
            .ok_or_else(|| anyhow!("No response_time_ms column found"))?;
        let (transaction_count_idx, _) = query_result
            .get_column_spec("transaction_count")
            .ok_or_else(|| anyhow!("No transaction_count column found"))?;
        let (block_proposal_count_idx, _) = query_result
            .get_column_spec("block_proposal_count")
            .ok_or_else(|| anyhow!("No block_proposal_count column found"))?;
        let (anomaly_score_idx, _) = query_result
            .get_column_spec("anomaly_score")
            .ok_or_else(|| anyhow!("No anomaly_score column found"))?;
        let (node_health_version_idx, _) = query_result
            .get_column_spec("node_health_version")
            .ok_or_else(|| anyhow!("No node_health_version column found"))?;

        let rows = query_result.rows.ok_or_else(|| anyhow!("No rows found"))?;

        if let Some(row) = rows.get(0) {
            let measured_peer_id = if let Some(CqlValue::Text(measured_peer_id)) = &row.columns[measured_peer_id_idx] {
                measured_peer_id.clone()
            } else {
                return Err(anyhow!("Unable to read measured_peer_id column"));
            };

            let peer_id = if let Some(CqlValue::Text(peer_id)) = &row.columns[peer_id_idx] {
                peer_id.clone()
            } else {
                return Err(anyhow!("Unable to read peer_id column"));
            };

            let epoch = if let Some(CqlValue::BigInt(epoch)) = row.columns[epoch_idx] {
                epoch as Epoch
            } else {
                return Err(anyhow!("Unable to read epoch column"));
            };

            let joined_epoch = if let Some(CqlValue::BigInt(joined_epoch)) = row.columns[joined_epoch_idx] {
                joined_epoch as Epoch
            } else {
                return Err(anyhow!("Unable to read joined_epoch column"));
            };

            let uptime_percentage = if let Some(CqlValue::Double(uptime_percentage)) = row.columns[uptime_percentage_idx] {
                uptime_percentage
            } else {
                return Err(anyhow!("Unable to read uptime_percentage column"));
            };

            let response_time_ms = if let Some(CqlValue::BigInt(response_time_ms)) = row.columns[response_time_ms_idx] {
                response_time_ms as u64
            } else {
                return Err(anyhow!("Unable to read response_time_ms column"));
            };

            let transaction_count = if let Some(CqlValue::BigInt(transaction_count)) = row.columns[transaction_count_idx] {
                transaction_count as u64
            } else {
                return Err(anyhow!("Unable to read transaction_count column"));
            };

            let block_proposal_count = if let Some(CqlValue::BigInt(block_proposal_count)) = row.columns[block_proposal_count_idx] {
                block_proposal_count as u64
            } else {
                return Err(anyhow!("Unable to read block_proposal_count column"));
            };

            let anomaly_score = if let Some(CqlValue::Double(anomaly_score)) = row.columns[anomaly_score_idx] {
                anomaly_score
            } else {
                return Err(anyhow!("Unable to read anomaly_score column"));
            };

            let node_health_version = if let Some(CqlValue::Int(node_health_version)) = row.columns[node_health_version_idx] {
                node_health_version as u32
            } else {
                return Err(anyhow!("Unable to read node_health_version column"));
            };

            Ok(Some(NodeHealth {
                measured_peer_id,
                peer_id,
                epoch,
                joined_epoch,
                uptime_percentage,
                response_time_ms,
                transaction_count,
                block_proposal_count,
                anomaly_score,
                node_health_version,
            }))
        } else {
            Ok(None)
        }
    }

    async fn update_node_health(&self, node_health: &NodeHealth) -> Result<(), Error> {
        self.update(node_health).await
    }

    async fn load_node_healths(&self, _epoch: Epoch) -> Result<Vec<NodeHealth>, Error> {
        Err(anyhow!("Not supported"))
    }

    async fn create_or_update(&self, _node_health: &NodeHealth) -> Result<(), Error> {
        Err(anyhow!("Not supported"))
    }
}