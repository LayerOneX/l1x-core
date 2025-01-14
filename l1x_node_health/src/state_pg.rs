use anyhow::Error;
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use db::postgres::{
    pg_models::{NewNodeHealth, QueryNodeHealth},
    postgres::{PgConnectionType, PostgresDBConn},
    schema,
};
use db_traits::{base::BaseState, node_health::NodeHealthState};
use diesel::{self, prelude::*, QueryResult};
use primitives::*;
use system::node_health::NodeHealth;
use util::convert::convert_to_big_decimal_epoch;
use bigdecimal::ToPrimitive;

pub struct StatePg<'a> {
    pub(crate) pg: &'a PostgresDBConn<'a>,
}

#[async_trait]
impl<'a> BaseState<NodeHealth> for StatePg<'a> {
    async fn create_table(&self) -> Result<(), Error> {
        // Implementation for create_table method
        Ok(())
    }

    async fn create(&self, node_health: &NodeHealth) -> Result<(), Error> {
        let new_node_health = NewNodeHealth {
            measured_peer_id: node_health.measured_peer_id.clone(),
            peer_id: node_health.peer_id.clone(),
            epoch: convert_to_big_decimal_epoch(node_health.epoch),
            joined_epoch: convert_to_big_decimal_epoch(node_health.joined_epoch),
            uptime_percentage: node_health.uptime_percentage,
            response_time_ms: BigDecimal::from(node_health.response_time_ms),
            transaction_count: BigDecimal::from(node_health.transaction_count),
            block_proposal_count: BigDecimal::from(node_health.block_proposal_count),
            anomaly_score: node_health.anomaly_score,
            node_health_version: node_health.node_health_version as i32,
        };

        let res = match &self.pg.conn {
            PgConnectionType::TxConn(conn) => diesel::insert_into(schema::node_health::table)
                .values(new_node_health)
                .execute(*conn.lock().await),
            PgConnectionType::PgConn(conn) => diesel::insert_into(schema::node_health::table)
                .values(new_node_health)
                .execute(&mut *conn.lock().await),
        };

        match res {
            Ok(_result) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Failed to create node health: {:?}", e)),
        }
    }

    async fn update(&self, node_health: &NodeHealth) -> Result<(), Error> {
        let res = match &self.pg.conn {
            PgConnectionType::TxConn(conn) => diesel::update(schema::node_health::table)
                .filter(schema::node_health::dsl::measured_peer_id.eq(&node_health.measured_peer_id))
                .filter(schema::node_health::dsl::epoch.eq(convert_to_big_decimal_epoch(node_health.epoch)))
                .set((
                    schema::node_health::dsl::uptime_percentage.eq(node_health.uptime_percentage),
                    schema::node_health::dsl::response_time_ms.eq(BigDecimal::from(node_health.response_time_ms)),
                    schema::node_health::dsl::transaction_count.eq(BigDecimal::from(node_health.transaction_count)),
                    schema::node_health::dsl::block_proposal_count.eq(BigDecimal::from(node_health.block_proposal_count)),
                    schema::node_health::dsl::anomaly_score.eq(node_health.anomaly_score),
                    schema::node_health::dsl::node_health_version.eq(node_health.node_health_version as i32),
                ))
                .execute(*conn.lock().await),
            PgConnectionType::PgConn(conn) => diesel::update(schema::node_health::table)
                .filter(schema::node_health::dsl::measured_peer_id.eq(&node_health.measured_peer_id))
                .filter(schema::node_health::dsl::epoch.eq(convert_to_big_decimal_epoch(node_health.epoch)))
                .set((
                    schema::node_health::dsl::uptime_percentage.eq(node_health.uptime_percentage),
                    schema::node_health::dsl::response_time_ms.eq(BigDecimal::from(node_health.response_time_ms)),
                    schema::node_health::dsl::transaction_count.eq(BigDecimal::from(node_health.transaction_count)),
                    schema::node_health::dsl::block_proposal_count.eq(BigDecimal::from(node_health.block_proposal_count)),
                    schema::node_health::dsl::anomaly_score.eq(node_health.anomaly_score),
                    schema::node_health::dsl::node_health_version.eq(node_health.node_health_version as i32),
                ))
                .execute(&mut *conn.lock().await),
        };

        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Failed to update node health: {:?}", e)),
        }
    }

    async fn raw_query(&self, query: &str) -> Result<(), Error> {
        match &self.pg.conn {
            PgConnectionType::TxConn(conn) => diesel::sql_query(query).execute(*conn.lock().await),
            PgConnectionType::PgConn(conn) => diesel::sql_query(query).execute(&mut *conn.lock().await),
        }?;
        Ok(())
    }

    async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
        // Implementation for set_schema_version method
        Ok(())
    }
}

#[async_trait]
impl<'a> NodeHealthState for StatePg<'a> {
    async fn store_node_health(&self, node_health: &NodeHealth) -> Result<(), Error> {
        self.create(node_health).await
    }

    async fn load_node_health(&self, peer_id: &str, epoch: Epoch) -> Result<Option<NodeHealth>, Error> {
        let epoch_decimal: BigDecimal = convert_to_big_decimal_epoch(epoch);
        let res: QueryResult<QueryNodeHealth> = match &self.pg.conn {
            PgConnectionType::TxConn(conn) => schema::node_health::table
                .filter(schema::node_health::dsl::peer_id.eq(peer_id))
                .filter(schema::node_health::dsl::epoch.eq(epoch_decimal))
                .first(*conn.lock().await),
            PgConnectionType::PgConn(conn) => schema::node_health::table
                .filter(schema::node_health::dsl::peer_id.eq(peer_id))
                .filter(schema::node_health::dsl::epoch.eq(epoch_decimal))
                .first(&mut *conn.lock().await),
        };

        match res {
            Ok(query_result) => Ok(Some(NodeHealth {
                measured_peer_id: query_result.measured_peer_id,
                peer_id: query_result.peer_id,
                epoch: query_result.epoch.to_u64().unwrap_or(0),
                joined_epoch: query_result.joined_epoch.to_u64().unwrap_or(0),
                uptime_percentage: query_result.uptime_percentage,
                response_time_ms: query_result.response_time_ms.to_u64().unwrap_or(0),
                transaction_count: query_result.transaction_count.to_u64().unwrap_or(0),
                block_proposal_count: query_result.block_proposal_count.to_u64().unwrap_or(0),
                anomaly_score: query_result.anomaly_score,
                node_health_version: query_result.node_health_version as u32,
            })),
            Err(diesel::NotFound) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to load node health: {:?}", e)),
        }
    }

    async fn update_node_health(&self, node_health: &NodeHealth) -> Result<(), Error> {
        self.update(node_health).await
    }

    async fn load_node_healths(&self, _epoch: Epoch) -> Result<Vec<NodeHealth>, Error> {
        let mut node_healths: Vec<NodeHealth> = vec![];
        let epoch_decimal: BigDecimal = convert_to_big_decimal_epoch(_epoch);
        let res: QueryResult<Vec<QueryNodeHealth>> = match &self.pg.conn {
            PgConnectionType::TxConn(conn) => schema::node_health::table
                .filter(schema::node_health::dsl::epoch.eq(epoch_decimal))
                .load(*conn.lock().await),
            PgConnectionType::PgConn(conn) => schema::node_health::table
                .filter(schema::node_health::dsl::epoch.eq(epoch_decimal))
                .load(&mut *conn.lock().await),
        };

        match res {
            Ok(query_result) => {
                for query_node_health in query_result {
                    let _node_health = NodeHealth {
                        measured_peer_id: query_node_health.measured_peer_id,
                        peer_id: query_node_health.peer_id,
                        epoch: query_node_health.epoch.to_u64().unwrap_or(0),
                        joined_epoch: query_node_health.joined_epoch.to_u64().unwrap_or(0),
                        uptime_percentage: query_node_health.uptime_percentage,
                        response_time_ms: query_node_health.response_time_ms.to_u64().unwrap_or(0),
                        transaction_count: query_node_health.transaction_count.to_u64().unwrap_or(0),
                        block_proposal_count: query_node_health.block_proposal_count.to_u64().unwrap_or(0),
                        anomaly_score: query_node_health.anomaly_score,
                        node_health_version: query_node_health.node_health_version as u32,
                    };
                    node_healths.push(_node_health);
                }
                Ok(node_healths)
            },
            Err(e) => Err(anyhow::anyhow!("Failed to load node health: {:?}", e)),
        }
    }

    async fn create_or_update(&self, _node_health: &NodeHealth) -> Result<(), Error> {
        // Check if an entry with the same epoch exists
        let epoch_decimal: BigDecimal = convert_to_big_decimal_epoch(_node_health.epoch);
        let existing_node_health: Option<QueryNodeHealth> = match &self.pg.conn {
            PgConnectionType::TxConn(conn) => schema::node_health::table
                .filter(schema::node_health::dsl::epoch.eq(epoch_decimal))
                .first(*conn.lock().await)
                .optional()
                .map_or(None, |res| res) ,
            PgConnectionType::PgConn(conn) => schema::node_health::table
                .filter(schema::node_health::dsl::epoch.eq(epoch_decimal))
                .first(&mut *conn.lock().await)
                .optional()
                .map_or(None, |res| res) ,
        };

        if let Some(_) = existing_node_health {
            let update_result = self.update(_node_health).await;
            update_result.map_err(|e| anyhow::anyhow!("Failed to update node health: {:?}", e))?;
        } else {
            let insert_result = self.create(_node_health).await;
            insert_result.map_err(|e| anyhow::anyhow!("Failed to create node health: {:?}", e))?;
        }

        Ok(())
    }
}