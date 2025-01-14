use anyhow::{anyhow, Error};
use async_trait::async_trait;
use db_traits::{base::BaseState, node_health::NodeHealthState};
use primitives::*;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use serde_json;
use system::node_health::NodeHealth;

pub struct StateRock {
    pub(crate) db_path: String,
    pub db: DBWithThreadMode<MultiThreaded>,
}

#[async_trait]
impl BaseState<NodeHealth> for StateRock {
    async fn create_table(&self) -> Result<(), Error> {
        // RocksDB doesn't require table creation
        Ok(())
    }

    async fn create(&self, node_health: &NodeHealth) -> Result<(), Error> {
        let key = format!("node_health:{}:{}:{}", node_health.measured_peer_id, node_health.peer_id, node_health.epoch);
        let value = serde_json::to_vec(node_health)?;
        self.db.put(key, value)?;
        Ok(())
    }

    async fn update(&self, _node_health: &NodeHealth) -> Result<(), Error> {
        // Implementation for update method
        Ok(())
    }

    async fn raw_query(&self, _query: &str) -> Result<(), Error> {
        // RocksDB doesn't support SQL-like queries
        Err(Error::msg("Raw queries are not supported in RocksDB"))
    }

    async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
        // Implementation for set_schema_version method
        Ok(())
    }
}

#[async_trait]
impl NodeHealthState for StateRock {
    async fn store_node_health(&self, node_health: &NodeHealth) -> Result<(), Error> {
        self.create(node_health).await
    }

    async fn load_node_health(&self, peer_id: &str, epoch: Epoch) -> Result<Option<NodeHealth>, Error> {
        let prefix = format!("node_health:*:{}:{}", peer_id, epoch);
        let mut iter = self.db.prefix_iterator(prefix);
        
        if let Some(Ok((_, value))) = iter.next() {
            let node_health: NodeHealth = serde_json::from_slice(&value)?;
            Ok(Some(node_health))
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