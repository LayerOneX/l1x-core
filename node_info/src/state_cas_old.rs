use anyhow::{anyhow, Error};
use db::cassandra::DatabaseManager;
use primitives::*;
use scylla::{Session, _macro_internal::CqlValue};
use std::{collections::HashMap, sync::Arc};
use system::{account::Account, node_info::NodeInfo};
pub struct NodeInfoState {
    pub session: Arc<Session>,
}

impl NodeInfoState {
    pub async fn new() -> Result<Self, Error> {
        let db_session = DatabaseManager::get_session().await?;
        let state = NodeInfoState { session: db_session.clone() };
        state.create_table().await?;
        Ok(state)
    }

    pub async fn create_table(&self) -> Result<(), Error> {
        match self
            .session
            .query(
                "CREATE TABLE IF NOT EXISTS node_info (
                    address blob,
                    cluster_address blob,
                    ip_address blob,
					metadata blob,
                    signature blob,
                    verifying_key blob,
                    PRIMARY KEY (address)
                );",
                &[],
            )
            .await
        {
            Ok(_) => {},
            Err(_) => return Err(anyhow!("Create table failed")),
        };
        Ok(())
    }

    pub async fn store_node_info(
        &self,
        cluster_address: &Address,
        node_info: &NodeInfo,
    ) -> Result<(), Error> {
        let address: Address = Account::address(&node_info.verifying_key)?;
        match self
            .session
            .query(
                "INSERT INTO node_info (address, cluster_address, ip_address, metadata, signature, verifying_key) VALUES (?, ?, ?, ?, ?, ?);",
                (&address, cluster_address, &node_info.data.ip_address, &node_info.data.metadata, &node_info.signature, &node_info.verifying_key),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Failed to store full node info - {}", e)),
        }
    }

    pub async fn update_node_info(
        &self,
        cluster_address: &Address,
        node_info: &NodeInfo,
    ) -> Result<(), Error> {
        let address: Address = Account::address(&node_info.verifying_key)?;
        match self
            .session
            .query(
                "UPDATE node_info SET cluster_address = ? , ip_address = ?, metadata = ? WHERE address = ?;",
                (cluster_address, &node_info.data.ip_address, &node_info.data.metadata, &address),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Failed to store full node info - {}", e)),
        }
    }

    pub async fn store_nodes(
        &self,
        clusters: &HashMap<Address, HashMap<Address, NodeInfo>>,
    ) -> Result<(), Error> {
        for (cluster_address, full_node_infos) in clusters {
            for (_address, node_info) in full_node_infos {
                self.store_node_info(cluster_address, node_info).await?;
            }
        }
        Ok(())
    }

    pub async fn find_node_info(
        &self,
        address: &Address,
    ) -> Result<(Option<Address>, Option<NodeInfo>), Error> {
        let (
            address, cluster_address, ip_address, metadata, signature, verifying_key
        ) = self
            .session
            .query("SELECT address, cluster_address, ip_address, metadata, signature, verifying_key  FROM node_info WHERE address = ? ;", (address,))
            .await?
            .single_row()?
            .into_typed::<(Address, Address, IpAddress, Metadata, SignatureBytes, VerifyingKeyBytes)>()?;

        let node_info = NodeInfo::new(
            address,
            ip_address,
            metadata,
            cluster_address.clone(),
            signature,
            verifying_key,
        );
        Ok((Some(cluster_address), Some(node_info)))
    }

    pub async fn load_node_info(&self, address: &Address) -> Result<NodeInfo, Error> {
        let (
            address, cluster_address, ip_address, metadata, signature, verifying_key
        ) = self
            .session
            .query("SELECT address, cluster_address, ip_address, metadata, signature, verifying_key FROM node_info WHERE address = ?;", (address,))
            .await?
            .single_row()?
            .into_typed::<(Address, Address, IpAddress, Metadata, SignatureBytes, VerifyingKeyBytes)>()?;

        let node_info = NodeInfo::new(
            address,
            ip_address,
            metadata,
            cluster_address.clone(),
            signature,
            verifying_key,
        );
        Ok(node_info)
    }

    pub async fn load_nodes(
        &self,
        cluster_address: &Address,
    ) -> Result<HashMap<Address, NodeInfo>, Error> {
        let rows = self
            .session
            .query("SELECT address, cluster_address, ip_address, metadata, signature, verifying_key FROM node_info WHERE cluster_address = ? ALLOW FILTERING;", (cluster_address,))
            .await?
            .rows()?;

        let mut cluster = HashMap::new();
        for row in rows {
            let (address, cluster_address, ip_address, metadata, signature, verifying_key) = row
                .into_typed::<(Address, Address, IpAddress, Metadata, SignatureBytes, VerifyingKeyBytes)>(
                )?;

            let node_info = NodeInfo::new(
                address,
                ip_address,
                metadata,
                cluster_address.clone(),
                signature,
                verifying_key,
            );
            cluster.insert(address, node_info);
        }

        Ok(cluster)
    }

    pub async fn remove_node_info(&self, address: &Address) -> Result<(), Error> {
        match self
            .session
            .query("DELETE FROM node_info WHERE address = ?;", (&address,))
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Failed to remove node info - {}", e)),
        }
    }
}