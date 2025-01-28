use std::collections::HashMap;
use anyhow::{anyhow, Error};
use block::block_state::BlockState;
use block_proposer::block_proposer_state::BlockProposerState;
use db::db::{Database, DbTxConn};
use l1x_node_health::NodeHealthState;
use l1x_vrf::common::SecpVRF;
use log::{debug, error, info, warn};
use node_info::node_info_state::NodeInfoState;
use primitives::*;
use secp256k1::{hashes::sha256, Message, PublicKey, SecretKey};
use system::{
	account::Account, block::{BlockPayload, L1xResponse, QueryBlockMessage}, block_proposer::BlockProposerPayload, mempool::ProcessMempool, network::{BroadcastNetwork, EventBroadcast}, node_health::{NodeHealth, NodeHealthPayload}, node_info::NodeInfo, vote::{Vote, VoteSignPayload}, vote_result::VoteResult
};
use tokio::sync::{broadcast, mpsc};
use validate::{
	validate_block_proposer::ValidateBlockProposer, validate_node_info::ValidateNodeInfo,
};
use validator::{validator_state::ValidatorState, validator_manager::ValidatorManager};
use crate::pending_blocks::PendingBlocks;
use l1x_htm::realtime_checks::RealTimeChecks;
use block_proposer::block_proposer_manager::BlockProposerManager;
use system::block_header::BlockHeader;
use system::block_proposer::BlockProposer;
use std::str::FromStr;
use libp2p::PeerId;
use execute::execute_block::ExecuteBlock;
use l1x_htm::INVALID_RESPONSE_TIME;
use runtime_config::RuntimeConfigCache;
use system::block::BlockType;
use system::validator::Validator;
use validate::validate_block::ValidateBlock;
use validate::validate_vote_result::ValidateVoteResult;
use vote_result::vote_result_state::VoteResultState;

pub struct Consensus {
	pub event_tx: broadcast::Sender<EventBroadcast>,
	pub node_event_tx: broadcast::Sender<EventData>,
	pub network_client_tx: mpsc::Sender<BroadcastNetwork>,
	pub mempool_tx: mpsc::Sender<ProcessMempool>,
	pub node_address: Address,
	pub pool_address: Address,
	pub cluster_address: Address,
	pub secret_key: SecretKey,
	pub verifying_key: PublicKey,
	pub multinode_mode: bool,
	pub pending_blocks: PendingBlocks,
	pub node_info: NodeInfo,
	pub real_time_checks: RealTimeChecks,
	node_health_reports: HashMap<String, Vec<NodeHealth>>,
}

impl<'a>  Consensus {
	pub fn new(
		event_tx: broadcast::Sender<EventBroadcast>,
		node_event_tx: broadcast::Sender<EventData>,
		network_client_tx: mpsc::Sender<BroadcastNetwork>,
		mempool_tx: mpsc::Sender<ProcessMempool>,
		node_address: Address,
		pool_address: Address,
		cluster_address: Address,
		secret_key: SecretKey,
		verifying_key: PublicKey,
		multinode_mode: bool,
		node_info: NodeInfo,
		real_time_checks: RealTimeChecks,
	) -> Consensus {
		Self {
			event_tx: event_tx.clone(),
			node_event_tx: node_event_tx.clone(),
			network_client_tx: network_client_tx.clone(),
			mempool_tx: mempool_tx.clone(),
			node_address,
			pool_address,
			cluster_address: cluster_address.clone(),
			secret_key,
			verifying_key,
			multinode_mode,
			pending_blocks: PendingBlocks::new(cluster_address, node_address, pool_address, secret_key, verifying_key,mempool_tx, event_tx, node_event_tx, network_client_tx),
			node_health_reports: HashMap::new(),
			real_time_checks,
			node_info,
		}
	}

	/// Receive and collect node health
	pub async fn receive_node_health(&mut self, node_healths: Vec<NodeHealth>, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastNodeHealth(node_healths.clone()))
			.await
		{
			warn!("Unable to broadcast node_healths to network_client_tx channel: {:?}", e)
		}

		log::debug!("Calling receive node health");
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
		let epoch = match node_healths.first() {
			Some(first) => first.epoch.clone(), // Clone the epoch if needed
			None => return Err(anyhow!("No node health data received").into()),
		};
		let block_proposer = match block_proposer_state.load_block_proposer(self.cluster_address, epoch).await? {
			Some(proposer) => proposer,
			None => return Err(anyhow!("No block proposer for epoch {}", epoch).into()),
		};
		if self.node_address == block_proposer.address {
			for node_health in node_healths {
				let measured_peer_id = node_health.measured_peer_id.clone();
				self.node_health_reports
					.entry(measured_peer_id)
					.or_insert_with(Vec::new)
					.push(node_health);
			}
		}

        Ok(())
    }

	/// Receive, verify and store node health
	pub async fn receive_signed_node_health(&mut self, signed_node_healths: Vec<NodeHealthPayload>, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastSignedNodeHealth(signed_node_healths.clone()))
			.await
		{
			warn!("Unable to broadcast signed_node_healths to network_client_tx channel: {:?}", e)
		}

		log::debug!("Calling receive signed node health");
		for signed_node_health in signed_node_healths {
			if signed_node_health.verify_signature().await.is_ok() {
				let node_health_state = NodeHealthState::new(&db_pool_conn).await?;
				let node_health = NodeHealth{
					measured_peer_id: signed_node_health.node_health.measured_peer_id,
					peer_id: signed_node_health.node_health.peer_id,
					epoch: signed_node_health.node_health.epoch,
					joined_epoch: signed_node_health.node_health.joined_epoch,
					uptime_percentage: signed_node_health.node_health.uptime_percentage,
					response_time_ms: signed_node_health.node_health.response_time_ms,
					transaction_count: signed_node_health.node_health.transaction_count,
					block_proposal_count: signed_node_health.node_health.block_proposal_count,
					anomaly_score: signed_node_health.node_health.anomaly_score,
					node_health_version: signed_node_health.node_health.node_health_version
				};
				node_health_state.store_node_health(&node_health).await?;
			} else {
				warn!("Invalid signature on signed node health: {:?}", signed_node_health.node_health.peer_id);
			}
		}
		info!("Stored node health successfully");
		Ok(())
    }

    pub async fn aggregate_and_broadcast_node_health(&mut self, db_pool_conn: &'a DbTxConn<'a>, epoch: u64) -> Result<(), Error> {
		debug!("Calling aggregate and broadcast node health");
        let node_health_state = NodeHealthState::new(&db_pool_conn).await?;
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
		let block_proposer = match block_proposer_state.load_block_proposer(self.cluster_address, epoch).await? {
			Some(proposer) => proposer,
			None => return Err(anyhow!("No block proposer for epoch {}", epoch).into()),
		};
		let mut signed_healths: Vec<NodeHealthPayload> = Vec::new();

		log::debug!("DEBUG: Node address: {:?}, Block proposer address: {:?}", self.node_address, block_proposer.address);
		if self.node_address == block_proposer.address {
			let aggregated_health = NodeHealth::aggregate_network_health(std::mem::take(&mut self.node_health_reports));
			log::debug!("DEBUG: Aggregated health: {:?}", aggregated_health);
			for (_, health) in aggregated_health {
				let json_str = serde_json::to_string(&health)?;
				let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());
				let sig = self.secret_key.sign_ecdsa(message);
				let signed_health = NodeHealthPayload {
					node_health: health.clone(),
					signature: sig.serialize_compact().to_vec(),
					verifying_key: self.verifying_key.serialize().to_vec(),
					sender: self.node_address,
				};
				log::debug!("DEBUG: Epoch: {:?}, Signed health: {:?}, Sender: {:?}", epoch, signed_health, self.node_address);
				signed_healths.push(signed_health);
				node_health_state.store_node_health(&health).await?;
			}

			log::debug!("DEBUG: Multinode mode: {:?}", self.multinode_mode);
			if self.multinode_mode {
				log::debug!("DEBUG: Sending BroadcastNetwork::BroadcastSignedNodeHealth event to network_client_tx channel, epoch: {:?}, signed_healths: {:?}", epoch, signed_healths);
				if let Err(e) = self.network_client_tx.send(BroadcastNetwork::BroadcastSignedNodeHealth(signed_healths)).await {
					warn!("Failed to broadcast aggregated node health: {:?}", e);
				}
			}
		}
		info!("Broadcasted aggregated node health");
        Ok(())
    }

	/// Validate the node info and store it in the database
	pub async fn receive_node_info(&mut self, node_info: NodeInfo, db_pool_conn: &'a DbTxConn<'a>,) -> Result<(), Error> {
		// Verify the signature of the node info
		ValidateNodeInfo::validate_node_info(&node_info).await?; // If there is any error it will return from here or else its a valid node info

		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastNodeInfo(node_info.clone()))
			.await
		{
			warn!("Unable to broadcast node_info to network_client_tx channel: {:?}", e)
		}

		// Only store if the node info is valid
		let node_info_state = NodeInfoState::new(&db_pool_conn).await?;
		node_info_state.upsert_node_info(&node_info).await?;
		Ok(())
	}

	/// Blocks are broadcast twice, once for validation (only to validators) and once when the block
	/// is finalized (to all the nodes). The network code hands off the just received block here
	pub async fn receive_validate_block(
		&mut self,
		block_payload: BlockPayload,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		debug!("Node has received new block from the network");
		{
			let block_state = BlockState::new(&db_pool_conn).await?;
			match block_state.is_block_executed(block_payload.block.block_header.block_number, &self.cluster_address).await {
				Ok(true) => {
					if let Err(e) = self
						.network_client_tx
						.send(BroadcastNetwork::BroadcastValidateBlock(block_payload)).await {
						warn!("Unable to write block to network_client_tx channel: {:?}", e)
					}
					return Ok(())
				}
				_ => ()
			}
		}
		// Always store the block because probably we have not received the parent block yet and can't validate this block
		self.pending_blocks.add_block(block_payload.clone());

		let block_payload = BlockPayload {
			block: block_payload.block,
			signature: block_payload.signature,
			verifying_key: block_payload.verifying_key,
			sender: self.node_address,
		};

		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastValidateBlock(block_payload))
			.await
		{
			warn!("Unable to write block to network_client_tx channel: {:?}", e)
		}

		self.pending_blocks.try_to_finalize(&db_pool_conn).await?;
		Ok(())
	}

	pub async fn receive_block_proposer(
		&mut self,
		block_proposer_payload: BlockProposerPayload,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		//verify signatures
		let _valid_block_proposer =
			ValidateBlockProposer::validate_block_proposer(&block_proposer_payload).await?;
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
		block_proposer_state
			.store_block_proposers(
				&block_proposer_payload.cluster_block_proposers.clone(),
				None,
				None,
			)
			.await?;

		// Broadcast block proposer if in multinode mode and the block proposer selection is valid
		if self.multinode_mode {
			// Update block_payload with node's address in order to make broadcast message unique for this node
			let block_proposer_payload = BlockProposerPayload {
				cluster_block_proposers: block_proposer_payload.cluster_block_proposers,
				signature: block_proposer_payload.signature,
				verifying_key: block_proposer_payload.verifying_key,
				sender: self.node_address,
			};
			if let Err(e) = self
				.network_client_tx
				.send(BroadcastNetwork::BroadcastBlockProposer(
					block_proposer_payload,
				))
				.await
			{
				warn!("Unable to write block_proposer to network_client_tx channel: {:?}", e)
			}
		}
		Ok(())
	}

	pub async fn receive_vote(&mut self, vote: Vote, db_pool_conn: &'a DbTxConn<'a>,) -> Result<(), Error> {
		let block_proposer_address = Account::address(&self.verifying_key.serialize().to_vec())?;

		let mut block_proposer_manager = BlockProposerManager {};
		let block_proposer = BlockProposer::new(
			vote.data.cluster_address,
			vote.data.epoch,
			block_proposer_address,
		);
		if !block_proposer_manager
			.is_block_proposer(
				block_proposer,
				&db_pool_conn,
			)
			.await? {
			return Ok(());
		}

		// check if block is already executed
		if !self.pending_blocks.get_blocks().contains_key(&vote.data.block_number) {
			let block_state = BlockState::new(&db_pool_conn).await?;
			match block_state.is_block_executed(vote.data.block_number, &self.cluster_address).await {
				Ok(true) => {
					return Ok(());
				}
				_ => ()
			}
		}
		self.pending_blocks.add_vote(vote.clone());

		let validator_state = ValidatorState::new(&db_pool_conn).await?;
		let validators = validator_state
			.load_all_validators(vote.data.epoch)
			.await?
			.ok_or(anyhow!("No validators selected for this epoch"))?;

		info!("Validators  #{:?}", validators);

		let vote_result = self.pending_blocks.try_to_vote_result(vote.data.block_number, &self.secret_key, &self.verifying_key, block_proposer_address, validators.clone())?;

		self.pending_blocks.add_vote_result(vote_result.clone());

		info!("Generated VoteResult for block #{}", vote.data.block_number);

		if self.multinode_mode {
			if let Err(e) = self
				.network_client_tx
				.send(BroadcastNetwork::BroadcastVoteResult(vote_result.clone()))
				.await
			{
				warn!("Unable to write generated vote result to network_client_tx channel: {:?}", e)
			}
			if let Err(e) = self.network_client_tx.send(BroadcastNetwork::BroadcastVote(vote)).await
			{
				warn!("Unable to write generated vote to network_client_tx channel: {:?}", e)
			}
			let _ = self.pending_blocks.try_to_finalize(&db_pool_conn).await;
		}
		Ok(())
	}

	pub async fn receive_vote_result(&mut self, vote_result: VoteResult, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		self.pending_blocks.add_vote_result(vote_result.clone());

		self.pending_blocks.try_to_finalize(&db_pool_conn).await?;
		if self.multinode_mode {
			if let Err(e) = self
				.network_client_tx
				.send(BroadcastNetwork::BroadcastVoteResult(vote_result))
				.await
			{
				warn!("Unable to write vote result to network_client_tx channel: {:?}", e)
			}
		}

		Ok(())
	}

	pub async fn receive_block(&mut self, block_payload: BlockPayload, vote_result: Option<VoteResult>, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		debug!("Node has received finalized block from the network");
		// Check if the block is already executed
		let block_state = BlockState::new(&db_pool_conn).await?;

		if block_state
			.is_block_executed(block_payload.block.block_header.block_number, &self.cluster_address)
			.await?
		{
			return Ok(());
		}

		// Validate the block
		let valid_block = ValidateBlock::validate_block(
			&block_payload,
			&db_pool_conn,
			self.cluster_address,
			block_payload.sender,
		)
			.await;

		match valid_block {
			Ok(_) => {

				// Store the block in the database
				block_state.store_block(block_payload.block.clone()).await?;

				// Add vote_result in the pending list
				if let Some(vote_result) = vote_result {
					// Validate vote result
					ValidateVoteResult::validate_vote_result(&vote_result, &block_payload.block, &db_pool_conn).await?;

					// store vote_result
					let vote_state = VoteResultState::new(&db_pool_conn).await?;
					vote_state.store_vote_result(&vote_result).await?;
				}

				// Remove transactions from the mempool
				for tx in &block_payload.block.transactions {
					self.mempool_tx.send(ProcessMempool::RemoveTrasaction(tx.clone())).await?;
				}

				// Execute the block and generate events
				let events =
					ExecuteBlock::execute_block(&block_payload.block, self.event_tx.clone(), &db_pool_conn).await?;
				self.broadcast_events(events);

				info!(
					"Block #{} has been executed and finalized",
					block_payload.block.block_header.block_number
				);
				self.pending_blocks.try_to_finalize(&db_pool_conn).await?;
			}
			Err(e) => warn!(
				"Received block #{} is not valid: {:?}",
				block_payload.block.block_header.block_number, e
			),
		}

		Ok(())
	}

	pub async fn handle_query_block_request(&self, request: QueryBlockMessage, db_pool_conn: &'a DbTxConn<'a>,) -> Result<L1xResponse, Error> {
		let block_state = BlockState::new(&db_pool_conn).await?;
		let vote_result_state = VoteResultState::new(&db_pool_conn).await?;

		match block_state.load_block(request.block_number, &request.cluster_address).await {
			Ok(block) => {
				let json_str = serde_json::to_string(&block)?;
				let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());
				let sig = self.secret_key.sign_ecdsa(message);
				let is_finalized = block_state.is_block_executed(request.block_number, &request.cluster_address).await?;
				let vote_result: Option<VoteResult> = if block.block_header.block_type != BlockType::SystemBlock {
					Some(vote_result_state.load_vote_result(&block.block_header.block_hash).await?)
				} else {
					None
				};

				let res =  L1xResponse::QueryBlock {
					block_payload: BlockPayload {
						block,
						signature:sig.serialize_compact().to_vec(),
						verifying_key: self.verifying_key.serialize().to_vec(),
						sender: Account::address(&self.verifying_key.serialize().to_vec())?,
					},
					is_finalized,
					vote_result,
				};
				Ok(res)
			},
			Err(e) => Ok(L1xResponse::QueryBlockError(e.to_string())),
		}
	}

	pub async fn handle_ping_result(&mut self, peer_id: String, is_success: bool, rtt: u64) {
		log::debug!("DEBUG: Adding ping result for peer: {:?}, Is success: {:?}, RTT: {:?}", peer_id, is_success, rtt);
        self.real_time_checks.add_check(peer_id, is_success, rtt);
    }

	pub async fn handle_ping_eligible_peers(&mut self, epoch: Epoch, peer_ids: Vec<String>) {
		self.real_time_checks.update_eligible_peers(epoch, peer_ids);
	}

	pub async fn process_health_update(&mut self, epoch: u64, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error>{
		// Find the maximum length in the vectors and resize all the reports with max length - 1

		log::debug!("DEBUG: Real time checks > online_checks: {:?}", self.real_time_checks.online_checks.clone());
		log::debug!("DEBUG: Real time checks > eligible_peers: {:?}", self.real_time_checks.eligible_peers.clone());
		let max_length = match self.real_time_checks.online_checks.values().map(Vec::len).max() {
			Some(len) if len > 1 => len,
			_ => return Err(anyhow!("Insufficient data for processing for epoch: {}", epoch)),
		};
		let target_length = max_length - 1;
		for (_, vec) in self.real_time_checks.online_checks.iter_mut() {
			if vec.len() < target_length {
				vec.resize(target_length, (false, INVALID_RESPONSE_TIME));
			}
		}

		let mut health_reports: Vec<NodeHealth> = Vec::new();
		let mut block_proposer_manager = BlockProposerManager {};
		let block_proposer = BlockProposer::new(
			self.cluster_address,
			epoch,
			self.node_address,
		);
		let is_block_proposer = block_proposer_manager.is_block_proposer(block_proposer, db_pool_conn).await?;
		log::debug!("DEBUG: Is block proposer: {:?}", is_block_proposer);
		log::debug!("DEBUG: Real time checks: {:?}", self.real_time_checks.online_checks.clone());

		for (measured_peer_id, _) in self.real_time_checks.online_checks.clone() {
			let health_report = match self.real_time_checks.create_health_report(
				measured_peer_id.clone(),
				self.node_info.peer_id.clone(),
				epoch,
				self.node_info.joined_epoch,
			) {
				Ok(report) => report,
				Err(e) => {
					log::error!("Failed to create health report for peer {}: {}", measured_peer_id, e);
					continue;
				}
			};

			log::debug!("DEBUG: Health report: {:?}", health_report);

			// Include health report into network health reports if node is a block proposer
			if is_block_proposer {
				self.node_health_reports
					.entry(measured_peer_id)
					.or_insert_with(Vec::new)
					.push(health_report.clone());
			}

			health_reports.push(health_report);
		}
		if let Err(e) = self.network_client_tx.send(BroadcastNetwork::BroadcastNodeHealth(health_reports)).await {
			log::error!("Failed to send OutboundNodeHealth event: {}", e);
		}

		// Clear checks and remove stale entries
		self.real_time_checks.clear_checks();

		Ok(())
	}

	pub fn broadcast_events(&self, events: Vec<EventData>) {
		for event in events {
			if let Err(err) = self.node_event_tx.send(event) {
				warn!("Failed to publish ReceiveBlock event due to {:?}", err);
			}
		}
	}

	pub async fn add_and_broadcast_block(&mut self, block_payload: BlockPayload) -> Result<(), Error> {
		let block_number = block_payload.block.block_header.block_number;
		let db_pool_conn = Database::get_pool_connection().await?;
		let block_state = BlockState::new(&db_pool_conn).await?;
		let is_block_stored = block_state.is_block_header_stored(block_number).await?;
		if is_block_stored {
			warn!("Drop Block #{} because it's already stored", block_number);
			// Block is already present
			return Err(anyhow!("Block #{} is already stored", block_number));
		}

		// check if we have any block present in the pending list
		if self.pending_blocks.get_blocks().len() == 1 {
			if let Some(pending_block) = self.pending_blocks.get_blocks().get(&block_number) {
				// Get block proposer address from the new block payload
				let block_proposer_address = Account::address(&block_payload.verifying_key)?;
				let pending_block_payload = pending_block.get_block().ok_or(anyhow::anyhow!("Failed to get block from the pending block"))?;
				// Get previous block proposer address from the pending block
				let previous_block_proposer = Account::address(&pending_block_payload.verifying_key)?;
				// Compare block proposer addresses and handle duplicate proposer (block will be replaced if proposed by different proposer)
				if block_proposer_address == previous_block_proposer {
					warn!("Received block: {} from same block proposer. Broadcasting the block again", block_number);
					// broadcast block again if same block is proposed by an existing block proposer
					self.broadcast_new_block(pending_block_payload.clone()).await?;

					let vote = pending_block.all_votes().into_iter().find(|v| v.verifying_key == block_payload.verifying_key);
					if let Some(vote) = vote {
						self.broadcast_vote(vote.clone()).await?;
					}
					return Err(anyhow!("Block already present in pending list"));
				}
			} else {
				let pending_blocks = self.pending_blocks.get_blocks().keys().collect::<Vec<_>>();
				// return error if a new block is proposed while a previous one is still pending
				return Err(anyhow!("Last block is not executed yet, #{}, pending blocks: {:?}", block_number, pending_blocks))
			}
		}

		let vote_sign_payload = VoteSignPayload::new(
			block_payload.block.block_header.block_number,
			block_payload.block.block_header.block_hash,
			self.cluster_address.clone(),
			block_payload.block.block_header.epoch,
			true, // aye vote is implicit as the node produced the block itself
		);

		let sig = vote_sign_payload.sign_with_ecdsa(self.secret_key)?;

		let vote = Vote::new(
			vote_sign_payload,
			self.node_address.clone(),
			sig.serialize_compact().to_vec(),
			self.verifying_key.serialize().to_vec(),
		);
		
		// broadcast block
		self.broadcast_new_block(block_payload.clone()).await?;

		// Add block to pending list
		self.pending_blocks.add_vote(vote);
		self.pending_blocks.add_block(block_payload);

		Ok(())
	}

	pub async fn broadcast_new_block(&self, block_payload: BlockPayload) -> Result<(), Error> {
		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastValidateBlock(block_payload))
			.await
		{
			warn!("Unable to write block to network_client_tx channel: {:?}", e)
		}
		Ok(())
	}

	pub async fn broadcast_vote(&self, vote: Vote) -> Result<(), Error> {
		if let Err(e) = self.network_client_tx.send(BroadcastNetwork::BroadcastVote(vote)).await
		{
			warn!("Unable to write vote to network_client_tx channel: {:?}", e)
		}
		Ok(())
	}

	pub async fn request_node_status(&self) -> Result<(), Error> {
		for peer_id in &self.real_time_checks.eligible_peers.1 {
			let peer_id = match PeerId::from_str(peer_id.as_str()) {
				Ok(peer_id) => peer_id,
				Err(e) => {
					log::error!("Unable to calculate peer_id: {:?}", e);
					continue;
				}
			};

			info!("DEBUB::TMP_27012025: Broadcasting query node status request to peer: {:?}", peer_id.clone());

			if let Ok(request_time) = util::generic::current_timestamp_in_millis() {

				info!("DEBUG_27012025: request_time: {:?}", request_time.clone());
				
				if let Err(e) = self
					.network_client_tx
					.send(BroadcastNetwork::BroadcastQueryNodeStatusRequest(request_time, peer_id)).await
				{
					warn!("Unable to write vote to network_client_tx channel: {:?}", e)
				}
			} else {
				error!("Unable to get current timestamp");
			}

		}
		Ok(())
	}
}

pub async fn select_and_store_validators_and_proposer<'a>(epoch: Epoch,
													 last_block_header: &BlockHeader,
													 db_pool_conn: &'a DbTxConn<'a>,
) -> Result<(), Error> {
	let rt_config = RuntimeConfigCache::get().await?;

	// Select block validators
	let validator_manager = ValidatorManager{};
	let selected_validators = validator_manager
		.select_validators_for_epoch(
			&last_block_header,
			epoch,
			db_pool_conn
		)
		.await?;

	let mut validator_print = String::new();
	for v in &selected_validators {
		let s = format!("\t{}\n", v);
		validator_print.push_str(&s);
	}
	info!(
			"Selected {} validators for epoch {}: \n{}",
			selected_validators.len(),
			epoch,
			validator_print
		);

	// Store selected validators in validator state
	let validator_state = ValidatorState::new(db_pool_conn).await?;
	validator_state.batch_store_validators(&selected_validators).await?;
	// Select block proposer
	let mut block_proposer_manager = BlockProposerManager{};
	let mut eligible_block_proposers: Vec<Validator> = vec![];
	// filter out validator based on whitelisted/blacklisted nodes
	if let Some(whitelisted_block_proposers) = &rt_config.whitelisted_block_proposers {
		eligible_block_proposers = selected_validators.into_iter()
			.filter(|v| (whitelisted_block_proposers.contains(&v.address))).collect();
	} else if let Some(blacklisted_block_proposers) = &rt_config.blacklisted_block_proposers {
		eligible_block_proposers = selected_validators.into_iter()
			.filter(|v| (!blacklisted_block_proposers.contains(&v.address))).collect();
	}
	
	let block_proposer = block_proposer_manager.select_block_proposers(epoch, last_block_header, eligible_block_proposers, db_pool_conn).await?;
	info!("Block proposer: {} selected for epoch: {}", hex::encode(block_proposer), epoch);
	Ok(())
}
