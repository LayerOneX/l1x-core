use crate::node_info_state::NodeInfoState;
use anyhow::Error;
use db::db::DbTxConn;
use l1x_vrf::common::SecpVRF;
use primitives::*;
use secp256k1::SecretKey;
use system::node_info::{NodeInfo, NodeInfoSignPayload};
pub struct NodeInfoManager {}

impl<'a> NodeInfoManager {
	pub async fn store_node_info(
		node_address: Address,
		node_ip_address: IpAddress, // eg. "/ip4/0.0.0.0/tcp/5010"
		peer_id: String, //  16Uiu2HAm5cnMkuwwmNyNdHNzxvhtHxKQ1xJtF7zb5Fpm9zbe7qmm
		metadata: Metadata,
		cluster_address: Address,
		joined_epoch: Epoch,
		secret_key: SecretKey,
		verifying_key_bytes: VerifyingKeyBytes,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		let node_info_payload = NodeInfoSignPayload {
			ip_address: node_ip_address.clone(),
			metadata: metadata.clone(),
			cluster_address: cluster_address.clone(),
		};

		let signature = node_info_payload.sign_with_ecdsa(secret_key).expect("Unable to sign");
		let node_info = NodeInfo::new(
			node_address.clone(),
			peer_id.clone(),
			joined_epoch.clone(),
			node_ip_address.clone(),
			metadata.clone(),
			cluster_address.clone(),
			signature.serialize_compact().to_vec(),
			verifying_key_bytes,
		);

		let node_info_state = NodeInfoState::new(db_pool_conn).await?;
		node_info_state.store_node_info(&node_info).await?;
		Ok(())
	}

	pub async fn update_node_info(
		&mut self,
		node_info: &NodeInfo,
		_cluster_address: &Address,
		node_info_state: &NodeInfoState<'a>,
	) -> Result<(), Error> {
		// Verify the signature
		node_info.verify_signature().await?;
		node_info_state.update_node_info(&node_info.clone()).await?;
		Ok(())
	}

	pub async fn join_cluster(
		&mut self,
		node_info: &NodeInfo,
		_cluster_address: &Address,
		node_info_state: &NodeInfoState<'a>,
	) -> Result<(), Error> {
		// Verify the signature
		node_info.verify_signature().await?;
		let (found_cluster_address, _) =
			match node_info_state.find_node_info(&node_info.address).await {
				Ok((found_cluster_address, node_info)) => (found_cluster_address, node_info),
				Err(_) => (None, None),
			};
		if found_cluster_address.is_none() {
			node_info_state.store_node_info(&node_info.clone()).await?;
		} else {
			node_info_state.update_node_info(&node_info.clone()).await?;
		}
		Ok(())
	}

	pub async fn find_cluster_address(
		&mut self,
		address: &Address,
		node_info_state: &NodeInfoState<'a>,
	) -> Result<Option<Address>, Error> {
		let (found_cluster_address, _) = node_info_state.find_node_info(address).await?;
		Ok(found_cluster_address)
	}

	pub async fn leave_cluster(
		&mut self,
		node_info: &NodeInfo,
		node_info_state: &NodeInfoState<'a>,
	) -> Result<(), Error> {
		// Verify the signature
		node_info.verify_signature().await?;
		node_info_state.remove_node_info(&node_info.address).await?;
		Ok(())
	}

	pub async fn update_cluster(
		&mut self,
		node_info: &NodeInfo,
		node_info_state: &NodeInfoState<'a>,
	) -> Result<(), Error> {
		// Verify the signature
		node_info.verify_signature().await?;
		let (found_cluster_address, _) = node_info_state.find_node_info(&node_info.address).await?;
		println!("found_cluster_address - {:?}", found_cluster_address.unwrap());
		if let Some(_found_cluster_address) = found_cluster_address {
			node_info_state.update_node_info(node_info).await?;
		}
		Ok(())
	}
}
