use anyhow::{anyhow, Error};
use async_trait::async_trait;
use bigdecimal::ToPrimitive;
use db_traits::{base::BaseState, node_info::NodeInfoState};
use primitives::*;
use scylla::Session;
use std::{collections::HashMap, sync::Arc};
use system::{account::Account, node_info::NodeInfo};

pub struct StateCas {
	pub(crate) session: Arc<Session>,
}

#[async_trait]
impl BaseState<NodeInfo> for StateCas {
	async fn create_table(&self) -> Result<(), Error> {
		match self
			.session
			.query(
				"CREATE TABLE IF NOT EXISTS node_info (
                    address blob,
					peer_id blob,
                    cluster_address blob,
                    ip_address blob,
					metadata blob,
					joined_epoch Bigint,
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

	async fn create(&self, node_info: &NodeInfo) -> Result<(), Error> {
		let address: Address = Account::address(&node_info.verifying_key)?;
		match self
			.session
			.query(
				"INSERT INTO node_info (address, peer_id, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key) VALUES (?, ?, ?, ?, ?, ?);",
				(&address, &node_info.peer_id, &node_info.joined_epoch.to_i64(), &node_info.data.cluster_address, &node_info.data.ip_address, &node_info.data.metadata, &node_info.signature, &node_info.verifying_key),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Failed to store full node info - {}", e)),
		}
	}

	async fn update(&self, node_info: &NodeInfo) -> Result<(), Error> {
		let address: Address = Account::address(&node_info.verifying_key)?;
		match self
			.session
			.query(
				"UPDATE node_info SET peer_id = ?, cluster_address = ?, ip_address = ?, metadata = ?, signature = ?, verifying_key = ? WHERE address = ?;",
				(&node_info.peer_id, &node_info.data.cluster_address, &node_info.data.ip_address, &node_info.data.metadata, &node_info.signature, &node_info.verifying_key, &address),
			)
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Failed to store full node info - {}", e)),
		}
	}

	async fn raw_query(&self, query: &str) -> Result<(), Error> {
		match self.session.query(query, &[]).await {
			Ok(_) => {},
			Err(e) => return Err(anyhow!("Failed to execute raw query: {}", e)),
		};
		Ok(())
	}

	async fn set_schema_version(&self, _version: u32) -> Result<(), Error> {
		todo!()
	}
}

#[async_trait]
impl NodeInfoState for StateCas {
	async fn store_nodes(
		&self,
		clusters: &HashMap<Address, HashMap<Address, NodeInfo>>,
	) -> Result<(), Error> {
		for full_node_infos in clusters.values() {
			for node_info in full_node_infos.values() {
				self.create(node_info).await?;
			}
		}
		Ok(())
	}

	async fn find_node_info(
		&self,
		address: &Address,
	) -> Result<(Option<Address>, Option<NodeInfo>), Error> {
		let (
			address, peer_id, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key
		) = self
			.session
			.query("SELECT address, peer_id, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key  FROM node_info WHERE address = ? ;", (address,))
			.await?
			.single_row()?
			.into_typed::<(Address, String, i64, Address, IpAddress, Metadata, SignatureBytes, VerifyingKeyBytes)>()?;
	
		let node_info = NodeInfo::new(
			address,
			peer_id,
			joined_epoch as u64,
			ip_address,
			metadata,
			cluster_address.clone(),
			signature,
			verifying_key,
		);
		Ok((Some(cluster_address), Some(node_info)))
	}
	async fn find_node_info_by_node_id(&self, node_id: Vec<u8>) -> Result<NodeInfo, Error> {
		let (
			address, peer_id, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key
		) = self
			.session
			.query("SELECT address, peer_id, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key FROM node_info WHERE address = ?;", (node_id,))
			.await?
			.single_row()?
			.into_typed::<(Address, String, i64, Address, IpAddress, Metadata, SignatureBytes, VerifyingKeyBytes)>()?;
	
		let node_info = NodeInfo::new(
			address,
			peer_id,
			joined_epoch as u64,
			ip_address,
			metadata,
			cluster_address.clone(),
			signature,
			verifying_key,
		);
		Ok(node_info)
	}

	async fn load_node_info(&self, address: &Address) -> Result<NodeInfo, Error> {
		let (
			address, peer_id, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key
		) = self
			.session
			.query("SELECT address, peer_id_id, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key FROM node_info WHERE address = ?;", (address,))
			.await?
			.single_row()?
			.into_typed::<(Address, String, i64, Address, IpAddress, Metadata, SignatureBytes, VerifyingKeyBytes)>()?;
	
		let node_info = NodeInfo::new(
			address,
			peer_id,
			joined_epoch as u64,
			ip_address,
			metadata,
			cluster_address.clone(),
			signature,
			verifying_key,
		);
		Ok(node_info)
	}

	async fn load_nodes(
		&self,
		cluster_address: &Address,
	) -> Result<HashMap<Address, NodeInfo>, Error> {
		let rows = self
			.session
			.query("SELECT address, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key FROM node_info WHERE cluster_address = ? ALLOW FILTERING;", (cluster_address,))
			.await?
			.rows()?;

		let mut cluster = HashMap::new();
		for row in rows {
			let (address, peer_id, joined_epoch, cluster_address, ip_address, metadata, signature, verifying_key) = row
				.into_typed::<(Address, String, i64, Address, IpAddress, Metadata, SignatureBytes, VerifyingKeyBytes)>(
				)?;

			let node_info = NodeInfo::new(
				address,
				peer_id,
				joined_epoch as u64,
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

	async fn remove_node_info(&self, address: &Address) -> Result<(), Error> {
		match self
			.session
			.query("DELETE FROM node_info WHERE address = ?;", (&address,))
			.await
		{
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Failed to remove node info - {}", e)),
		}
	}

	async fn has_address_exists(&self, _address: &Address) -> Result<bool, Error> {
		todo!()
	}

	async fn create_or_update(&self, _node_info: &NodeInfo) -> Result<(), Error> {
		Err(anyhow!("Not supported"))
	}
}
