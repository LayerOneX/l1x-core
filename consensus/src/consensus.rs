use std::collections::HashMap;
use anyhow::{anyhow, Error};
use block::{block_manager::BlockManagerCache, block_state::BlockState};
use block_proposer::block_proposer_state::BlockProposerState;
use db::db::{Database, DbTxConn};
use l1x_node_health::NodeHealthState;
use l1x_vrf::common::SecpVRF;
use log::{debug, error, info, warn};
use node_info::node_info_state::NodeInfoState;
use p2p::network::{NetworkState, PeerStatusInfo};
use primitives::*;
use secp256k1::{hashes::sha256, Message, PublicKey, SecretKey};
use system::{
	account::Account, block::{BlockPayload, L1xResponse, QueryBlockMessage}, block_proposer::BlockProposerPayload, mempool::ProcessMempool, network::{BroadcastNetwork, EventBroadcast}, node_health::{NodeHealth, NodeHealthPayload}, node_info::NodeInfo, node_status::NodeDetailedStatus, validator::ValidatorPayload, vote::{Vote, VoteSignPayload}, vote_result::VoteResult
};
use tokio::sync::{broadcast, mpsc};
use validate::{
	validate_block_proposer::ValidateBlockProposer, 
	validate_node_info::ValidateNodeInfo,
	validate_validator::ValidateValidatorPayload,
};
use validator:: validator_state::ValidatorState;
// use validator::{validator_state::ValidatorState, validator_manager::ValidatorManager};
use crate::pending_blocks::PendingBlocks;
use l1x_htm::realtime_checks::RealTimeChecks;
use block_proposer::block_proposer_manager::BlockProposerManager;
// use system::block_header::BlockHeader;
use system::block_proposer::BlockProposer;
use std::str::FromStr;
use libp2p::PeerId;
use execute::execute_block::ExecuteBlock;
use l1x_htm::INVALID_RESPONSE_TIME;
// use runtime_config::RuntimeConfigCache;
use system::block::BlockType;
// use system::validator::Validator;
use validate::validate_block::ValidateBlock;
use validate::validate_vote_result::ValidateVoteResult;
use vote_result::vote_result_state::VoteResultState;
// use std::time::{SystemTime,UNIX_EPOCH};
// const PENDING_BLOCK_EXPIRATION_TIME: u128 = 20 * 1000; //ms , 20 seconds
// const REBROADCAST_PENDING_THRESHOLD_MS: u128 = 30_000; // Example: 30 seconds

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
			warn!("🏛 ⚠️  Consensus -  Receive Node Health - Unable to broadcast node_healths to network_client_tx channel: {:?}", e)
		}

		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
		let epoch = match node_healths.first() {
			Some(first) => first.epoch.clone(), // Clone the epoch if needed
			None => return Err(anyhow!("🏛 ⚠️  Consensus -  Receive Node Health - No node health data received").into()),
		};
		let block_proposer = match block_proposer_state.load_block_proposer(self.cluster_address, epoch).await? {
			Some(proposer) => proposer,
			None => return Err(anyhow!("🏛 ⚠️  Consensus -  Receive Node Health - No block proposer for epoch {}", epoch).into()),
		};

		debug!("🏛 Consensus -  Receive Node Health - Node address: {:?}, Block proposer address: {:?}", self.node_address, block_proposer.address);
		if self.node_address == block_proposer.address {

			debug!("🏛 Consensus -  Receive Node Health - Storing node health reports");
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
			warn!("🏛 ⚠️  Consensus -  Receive Signed Node Health - Unable to broadcast signed_node_healths to network_client_tx channel: {:?}", e)
		}

		for signed_node_health in signed_node_healths {
			let debug_peer_id = signed_node_health.node_health.peer_id.clone();
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
				debug!("🏛 Consensus -  Receive Signed Node Health - Stored node health successfully for peer_id: {:?}", debug_peer_id);
			} else {
				warn!("🏛 ⚠️  Consensus -  Receive Signed Node Health - Invalid signature on signed node health: {:?}", debug_peer_id);
			}
		}
		info!("🏛 Consensus -  Recieved Signed Node Health - Stored node health successfully");
		Ok(())
    }

    pub async fn aggregate_and_broadcast_node_health(&mut self, db_pool_conn: &'a DbTxConn<'a>, epoch: u64) -> Result<(), Error> {
		debug!("🏛 Consensus -  Aggregate and Broadcast Node Health - Aggregating Node Health for Epoch: {}", epoch);
        let node_health_state = NodeHealthState::new(&db_pool_conn).await?;
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
		let block_proposer = match block_proposer_state.load_block_proposer(self.cluster_address, epoch).await? {
			Some(proposer) => proposer,
			None => return Err(anyhow!("🏛 Consensus -  Aggregate and Broadcast Node Health - No block proposer for epoch {}", epoch).into()),
		};
		let mut signed_healths: Vec<NodeHealthPayload> = Vec::new();

		debug!("🏛 Consensus -  Aggregate and Broadcast Node Health - Node address: {:?}, Block proposer address: {:?}", self.node_address, block_proposer.address);
		if self.node_address == block_proposer.address {
			let aggregated_health = NodeHealth::aggregate_network_health(std::mem::take(&mut self.node_health_reports));
			debug!("🏛 Consensus -  Aggregate and Broadcast Node Health - Aggregated health: {:?}", aggregated_health);
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
				debug!("🏛 Consensus -  Aggregate and Broadcast Node Health - Epoch: {:?}, Signed health: {:?}, Sender: {:?}", epoch, signed_health, self.node_address);
				signed_healths.push(signed_health);
				node_health_state.store_node_health(&health).await?;
			}

			debug!("🏛 Consensus -  Aggregate and Broadcast Node Health - Multinode mode: {:?}", self.multinode_mode);
			if self.multinode_mode {
				debug!("🏛 Consensus -  Aggregate and Broadcast Node Health - Sending BroadcastNetwork::BroadcastSignedNodeHealth event to network_client_tx channel, epoch: {:?}, signed_healths: {:?}", epoch, signed_healths);
				if let Err(e) = self.network_client_tx.send(BroadcastNetwork::BroadcastSignedNodeHealth(signed_healths)).await {
					warn!("🏛 ⚠️  Consensus -  Aggregate and Broadcast Node Health - Failed to broadcast aggregated node health: {:?}", e);
				}
			}
		}
		info!("🏛 Consensus -  Aggregate and Broadcast Node Health - Broadcasted aggregated node health");
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
			warn!("🏛 ⚠️  Consensus -  Receive Node Info - Unable to broadcast node_info to network_client_tx channel: {:?}", e)
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
		let block_number = block_payload.block.block_header.block_number;
		debug!("🏛 Consensus -  Recieved Validate Block - Node has received new block #{} from the network", block_number);
	
		// First check if block is already executed to avoid duplicate work
		let block_state = BlockState::new(&db_pool_conn).await?;
		if let Ok(true) = block_state.is_block_executed(block_number, &self.cluster_address).await {
			info!("🏛 Consensus -  Recieved Validate Block - Block #{} is already executed, skipping validation", block_number);
			// Still broadcast the block to ensure network-wide propagation
			if let Err(e) = self
				.network_client_tx
				.send(BroadcastNetwork::BroadcastValidateBlock(block_payload)).await {
				warn!("🏛 ⚠️  Consensus -  Recieved Validate Block - Unable to write block to network_client_tx channel: {:?}", e)
			}
			return Ok(());
		}
	
		// Validate the block proposer
		let current_epoch = block_payload.block.block_header.epoch;
		let block_proposer_state = BlockProposerState::new(db_pool_conn).await?;
		
		// Check for proposer information - handle missing proposer cases explicitly
		match block_proposer_state.load_block_proposer(self.cluster_address, current_epoch).await {
			Ok(Some(authorized_proposer)) => {
				// Derive proposer address from the block's verifying key
				match Account::address(&block_payload.verifying_key) {
					Ok(proposer_from_key) => {
						if proposer_from_key != authorized_proposer.address {
							warn!("🏛 ⚠️  Consensus -  Recieved Validate Block - Block #{} from unauthorized proposer {} (expected: {})",
								block_number,
								hex::encode(proposer_from_key),
								hex::encode(authorized_proposer.address)
							);
							
							// Note: In some cases this might be due to race conditions during proposer changes
							// Forward the block anyway to ensure network consistency, but don't add to pending
							if let Err(e) = self
								.network_client_tx
								.send(BroadcastNetwork::BroadcastValidateBlock(block_payload))
								.await
							{
								warn!("🏛 ⚠️  Consensus -  Recieved Validate Block - Unable to forward unauthorized block: {:?}", e)
							}
							return Ok(());
						}
						else {
							debug!("🏛 Consensus -  Recieved Validate Block - Block #{} from authorized proposer {}", block_number, hex::encode(proposer_from_key));
						}
					},
					Err(e) => {
						warn!("🏛 ⚠️  Consensus -  Recieved Validate Block - Failed to derive address from verifying key for block #{}: {}", 
							  block_number, e);
						return Ok(());
					}
				}
			},
			Ok(None) => {
				warn!("🏛 ⚠️  Consensus -  Recieved Validate Block - No authorized proposer found for epoch {} while validating block #{}", 
					  current_epoch, block_number);
				// We don't validate further due to missing proposer info
			},
			Err(e) => {
				warn!("🏛 ⚠️  Consensus -  Recieved Validate Block - Error loading block proposer for epoch {} while validating block #{}: {}", 
					  current_epoch, block_number, e);
				// We don't validate further due to proposer loading error
			}
		}
	
		// If we reach here, either the block has passed validation or we're skipping validation
		// due to missing proposer information
		self.pending_blocks.add_block(block_payload.clone());
		info!("🏛 Consensus -  Recieved Validate Block - Added block #{} to pending blocks queue", block_number);
	
		// Prepare for broadcasting - create a new payload with our node as the sender
		let block_payload_to_broadcast = BlockPayload {
			block: block_payload.block,
			signature: block_payload.signature,
			verifying_key: block_payload.verifying_key,
			sender: self.node_address,
		};
	
		// Broadcast to network
		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastValidateBlock(block_payload_to_broadcast))
			.await
		{
			warn!("🏛 ⚠️  Consensus -  Recieved Validate Block - Unable to write block to network_client_tx channel: {:?}", e)
		}
	
		// Try to finalize any pending blocks that may now be ready
		match self.pending_blocks.try_to_finalize(&db_pool_conn, self.pool_address).await {
			Ok(_) => debug!("🏛 Consensus -  Recieved Validate Block - Successfully checked pending blocks after receiving block"),
			Err(e) => {
				warn!("🏛 ⚠️  Consensus -  Recieved Validate Block - Error while finalizing pending blocks after receiving block #{}: {}", 
					  block_number, e)
			}
		}
		
		Ok(())
	}

	pub async fn receive_block_proposer(
		&mut self,
		block_proposer_payload: BlockProposerPayload,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		info!("🏛 Consensus -  Recieved Block Proposer from Network for epoch: {}, block proposer address: {}", block_proposer_payload.epoch, hex::encode(block_proposer_payload.block_proposer_address));
		//verify signatures

		debug!("🏛 Consensus -  Recieved Block Proposer - Validating block proposer payload: {:?}", block_proposer_payload);
		ValidateBlockProposer::validate_block_proposer(&block_proposer_payload).await?;

		// Check if 
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
		
		// Check if the sender is the block proposer for previous epoch
		let previous_epoch = block_proposer_payload.epoch - 1;
		let previous_block_proposer = match block_proposer_state.load_block_proposer(block_proposer_payload.cluster_address, previous_epoch).await? {
			Some(proposer) => proposer,
			None => return Err(anyhow!("🏛 ⚠️  Consensus -  Recieved Block Proposer - No block proposer found for previous epoch: {}", previous_epoch).into()),
		};
		if previous_block_proposer.address != block_proposer_payload.sender {
			warn!("🏛 Consensus -  Recieved Block Proposer - Sender address: {} is not the block proposer for previous epoch: {}", hex::encode(block_proposer_payload.sender), previous_epoch);
			return Ok(());
		}
		
		// Check if the block proposer is already stored
		if block_proposer_state.is_block_proposer_stored(block_proposer_payload.cluster_address, block_proposer_payload.epoch).await? {
			info!("🏛 Consensus -  Recieved Block Proposer - Block proposer is already stored for epoch: {}", block_proposer_payload.epoch);
			return Ok(());
		}

		block_proposer_state.store_block_proposer(block_proposer_payload.cluster_address, block_proposer_payload.epoch, block_proposer_payload.block_proposer_address).await?;
		
		Ok(())
	}

	pub async fn receive_validators(
		&mut self,
		validator_payload: ValidatorPayload,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		info!("🏛 Consensus -  Recieved Validators - Received Validators from Network for epoch: {}", validator_payload.epoch);
		debug!("🏛 Consensus -  Recieved Validators - Validating validator payload: {:?}", validator_payload);
		
		// Verify the signature of the validator payload
		ValidateValidatorPayload::validate_validator_payload(&validator_payload).await?;

		// Check if the sender is the block proposer for previous epoch
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
		let previous_epoch = validator_payload.epoch - 1;
		let previous_block_proposer = match block_proposer_state.load_block_proposer(validator_payload.cluster_address, previous_epoch).await? {
			Some(proposer) => proposer,
			None => return Err(anyhow!("🏛 ⚠️  Consensus -  Recieved Block Proposer - No block proposer found for previous epoch: {}", previous_epoch).into()),
		};
		if previous_block_proposer.address != validator_payload.sender {
			warn!("🏛 Consensus -  Recieved Validators - Sender address: {} is not the block proposer for previous epoch: {}", hex::encode(validator_payload.sender), previous_epoch);
			return Ok(());
		}


		// Store the validators in the database
		let validator_state = ValidatorState::new(&db_pool_conn).await?;

		// Check if the validators are already stored
		if validator_state.has_validators_for_epoch(validator_payload.epoch).await? {
			info!("🏛 Consensus -  Recieved Validators - Validators are already stored for epoch: {}", validator_payload.epoch);
			return Ok(());
		}

		validator_state.batch_store_validators(&validator_payload.validators).await?;

		
		Ok(())
	}

	pub async fn receive_vote(&mut self, vote: Vote, db_pool_conn: &'a DbTxConn<'a>,) -> Result<(), Error> {

		debug!("🏛 Consensus -  Recieved Vote - Received vote from network_client_tx channel, Block #: {:?}, Validator address: {:?}", vote.data.block_number, hex::encode(vote.validator_address));

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

		let all_votes = self.pending_blocks.all_votes(vote.data.block_number);
		debug!("🏛 Consensus -  Recieved Vote - All votes for Block #:{}, Votes Length: {:?}", vote.data.block_number, all_votes.len());
		for v in all_votes {
			debug!("🏛 Consensus -  Recieved Vote - Vote Details ~ Block #: {:?}, Validator Address: {:?}", v.data.block_number, hex::encode(v.validator_address));
		}

		let validator_state = ValidatorState::new(&db_pool_conn).await?;
		let validators = validator_state
			.load_all_validators(vote.data.epoch)
			.await?
			.ok_or(anyhow!("🏛 ⚠️  Consensus -  Recieved Vote - No validators selected for this epoch"))?;

		let validators_str = validators.iter().map(|v| hex::encode(v.address)).collect::<Vec<String>>().join(", ");

		info!("🏛 Consensus -  Recieved Vote - Selected Validators for Block #{}: , Epoch: {:?}, Validators: {:?}", vote.data.block_number, vote.data.epoch, validators_str);

		let vote_result = self.pending_blocks.try_to_vote_result(vote.data.block_number, &self.secret_key, &self.verifying_key, block_proposer_address, validators.clone(), db_pool_conn, self.pool_address).await?;

		self.pending_blocks.add_vote_result(vote_result.clone());

		info!("🏛 Consensus -  Recieved Vote - Generated VoteResult for block #{}, Validator address: {:?}", vote.data.block_number, hex::encode(vote_result.validator_address));
		

		if self.multinode_mode {
			if let Err(e) = self
				.network_client_tx
				.send(BroadcastNetwork::BroadcastVoteResult(vote_result.clone()))
				.await
			{
				warn!("🏛 ⚠️  Consensus -  Recieved Vote - Unable to write generated vote result to network_client_tx channel: {:?}", e)
			}
			if let Err(e) = self.network_client_tx.send(BroadcastNetwork::BroadcastVote(vote)).await
			{
				warn!("🏛 ⚠️  Consensus -  Recieved Vote - Unable to write generated vote to network_client_tx channel: {:?}", e)
			}
			let _ = self.pending_blocks.try_to_finalize(&db_pool_conn, self.pool_address).await;
		}
		Ok(())
	}

	pub async fn receive_vote_result(&mut self, vote_result: VoteResult, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		self.pending_blocks.add_vote_result(vote_result.clone());

		self.pending_blocks.try_to_finalize(&db_pool_conn, self.pool_address).await?;
		if self.multinode_mode {
			if let Err(e) = self
				.network_client_tx
				.send(BroadcastNetwork::BroadcastVoteResult(vote_result))
				.await
			{
				warn!("🏛 ⚠️  Consensus -  Recieved Vote Result - Unable to write vote result to network_client_tx channel: {:?}", e)
			}
		}

		Ok(())
	}

	pub async fn receive_block(&mut self, block_payload: BlockPayload, vote_result: Option<VoteResult>, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		debug!("🏛 Consensus -  Recieved Block - Node has received finalized Block #{} from the network", block_payload.block.block_header.block_number);
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

				info!("🏛 Consensus -  Recieved Block - Block #{} has been executed and finalized", block_payload.block.block_header.block_number
				);
				self.pending_blocks.try_to_finalize(&db_pool_conn, self.pool_address).await?;
			}
			Err(e) => warn!(
				"🏛 ⚠️  Consensus -  Recieved Block - Block #{} is not valid: {:?}",
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
		debug!("🏛 Consensus -  Handle Ping Result - Adding ping result for peer: {:?}, Is success: {:?}, RTT: {:?}", peer_id, is_success, rtt);
        self.real_time_checks.add_check(peer_id, is_success, rtt);
    }

	pub async fn handle_ping_eligible_peers(&mut self, epoch: Epoch, peer_ids: Vec<String>) {
		self.real_time_checks.update_eligible_peers(epoch, peer_ids);
	}

	pub async fn process_health_update(&mut self, epoch: u64, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error>{
		// Find the maximum length in the vectors and resize all the reports with max length - 1

		debug!("🏛 Consensus -  Process Health Update - Real time checks > online_checks: {:?} for epoch: {}", self.real_time_checks.online_checks.clone(), epoch);
		debug!("🏛 Consensus -  Process Health Update - Real time checks > eligible_peers: {:?} for epoch: {}", self.real_time_checks.eligible_peers.clone(), epoch);
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
		debug!("🏛 Consensus -  Process Health Update - Is block proposer: {:?}", is_block_proposer);

		for (measured_peer_id, _) in self.real_time_checks.online_checks.clone() {
			let health_report = match self.real_time_checks.create_health_report(
				measured_peer_id.clone(),
				self.node_info.peer_id.clone(),
				epoch,
				self.node_info.joined_epoch,
			) {
				Ok(report) => report,
				Err(e) => {
					error!("🏛 🚨  Consensus -  Process Health Update - Failed to create health report for peer {}: {}", measured_peer_id, e);
					continue;
				}
			};

			debug!("🏛 Consensus -  Process Health Update - Health report: {:?}", health_report);

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
			error!("🏛 🚨  Consensus -  Process Health Update - Failed to send OutboundNodeHealth event: {}", e);
		}

		// Clear checks and remove stale entries
		self.real_time_checks.clear_checks();

		Ok(())
	}

	pub fn broadcast_events(&self, events: Vec<EventData>) {
		for event in events {
			if let Err(err) = self.node_event_tx.send(event) {
				warn!("🏛 ⚠️  Consensus -  Broadcast Events - Failed to publish ReceiveBlock event due to {:?}", err);
			}
		}
	}

	pub async fn add_and_broadcast_block(&mut self, block_payload: BlockPayload) -> Result<(), Error> {
		let block_number = block_payload.block.block_header.block_number;
		let db_pool_conn = Database::get_pool_connection().await?;
		
		// Add Block proposer validation
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
		let current_epoch = block_payload.block.block_header.epoch;
		let authorized_block_proposer = block_proposer_state.load_block_proposer(self.cluster_address, current_epoch).await?.ok_or(anyhow!("No authorized block proposer for epoch : {}", current_epoch))?;
	
		debug!("🏛 Consensus -  Add and Broadcast Block - Authorized block proposer: {:?}", hex::encode(authorized_block_proposer.address));
		debug!("🏛 Consensus -  Add and Broadcast Block - Node address: {:?}", hex::encode(self.node_address));
		debug!("🏛 Consensus -  Add and Broadcast Block - Block number: {}, Epoch: {}", block_number, current_epoch);
	
		if authorized_block_proposer.address != self.node_address {
			return Err(anyhow!("🏛 Consensus -  Add and Broadcast Block - Node is not an authorized block proposer for epoch: {}", current_epoch));
		}
	
		// Check if block is already stored
		let block_state = BlockState::new(&db_pool_conn).await?;
		let is_block_stored = block_state.is_block_header_stored(block_number).await?;
		if is_block_stored {
			warn!("🏛 ⚠️ Consensus -  Add and Broadcast Block - Block #{} is already stored", block_number);
			return Err(anyhow!("🏛 Consensus -  Add and Broadcast Block - Block #{} is already stored", block_number));
		}
	
		// Check pending blocks
		if self.pending_blocks.get_blocks().len() == 1 {
			if let Some(pending_block) = self.pending_blocks.get_blocks().get(&block_number) {
				let block_proposer_address = Account::address(&block_payload.verifying_key)?;
				let pending_block_payload = pending_block.get_block().ok_or(anyhow!("Failed to get block from the pending block"))?;
				let previous_block_proposer = Account::address(&pending_block_payload.verifying_key)?;
				
				if block_proposer_address == previous_block_proposer {
					warn!("🏛 ⚠️ Consensus -  Add and Broadcast Block - Received block: {} from same block proposer. Broadcasting the block again", block_number);
					self.broadcast_new_block(pending_block_payload.clone()).await?;
					
					if let Some(vote) = pending_block.all_votes().into_iter().find(|v| v.verifying_key == block_payload.verifying_key) {
						self.broadcast_vote(vote.clone()).await?;
					}
					return Err(anyhow!("🏛 Consensus -  Add and Broadcast Block - Block already present in pending list"));
				}
			} else {
				let pending_blocks = self.pending_blocks.get_blocks().keys().collect::<Vec<_>>();
				warn!("🏛 ⚠️ Consensus -  Add and Broadcast Block - Last block is not executed yet, #{}, pending blocks: {:?}", block_number, pending_blocks);
				return Err(anyhow!("🏛 Consensus -  Add and Broadcast Block - Last block is not executed yet, #{}, pending blocks: {:?}", block_number, pending_blocks));
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
	
		debug!("🏛 Consensus -  Add and Broadcast Block - Adding Vote and Block to pending list, Block #: {:?}, Voter address: {:?}", 
			block_payload.block.block_header.block_number, 
			hex::encode(vote.validator_address));
	
		// Add block to pending list
		self.pending_blocks.add_vote(vote);
		self.pending_blocks.add_block(block_payload);
	
		info!("🏛 Consensus -  Add and Broadcast Block - Successfully added block #{} to pending state", block_number);
	
		Ok(())
	}

	pub async fn add_new_block_proposer(&mut self, block_proposer_payload: BlockProposerPayload) -> Result<(), Error> {
		let db_pool_conn = Database::get_pool_connection().await?;
		let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;

		// Check if already exists 
		debug!("🏛 Consensus -  Add New Block Proposer - Checking if block proposer is already stored for epoch: {}", block_proposer_payload.epoch);
		let is_stored = match block_proposer_state.is_block_proposer_stored(block_proposer_payload.cluster_address, block_proposer_payload.epoch).await? {
			true => true,
			false => false,
		};

		if !is_stored {
			warn!("🏛 Consensus -  Add New Block Proposer - Block proposer for epoch: {} is already stored", block_proposer_payload.epoch);
			// Store the new block proposer in the database	
			block_proposer_state.store_block_proposer(block_proposer_payload.cluster_address,block_proposer_payload.epoch, block_proposer_payload.block_proposer_address).await?;
		}

		
		
		// TODO: Broadcast the new block proposer to the network
		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastBlockProposer(block_proposer_payload.clone()))
			.await
		{
			warn!("🏛 ⚠️  Consensus -  Broadcast New Block - Unable to write block to network_client_tx channel: {:?}", e)
		}
		else
		{
			info!("🏛 Consensus -  Add New Block Proposer - Successfully broadcasted block proposer for epoch {}", block_proposer_payload.epoch);
		}

		
		Ok(())
	}

	pub async fn add_new_validators(&mut self, validator_payload: ValidatorPayload) -> Result<(), Error> {
		let db_pool_conn = Database::get_pool_connection().await?;
		let validator_state = ValidatorState::new(&db_pool_conn).await?;

		// Check if the validators are already stored
		debug!("🏛 Consensus -  Add New Validators - Checking if validators are stored for epoch: {}", validator_payload.epoch);
		let is_stored = match validator_state.has_validators_for_epoch(validator_payload.epoch).await{
			Ok(is_stored) => is_stored,
			Err(e) => {
				warn!("🏛 ⚠️  Consensus -  Add New Validators - Failed to check if validators are stored: {:?}", e);
				return Err(e);
			}
		};

		if !is_stored {
			warn!("🏛 Consensus -  Add New Validators - Validators for epoch: {} are already stored", validator_payload.epoch);
			// Store the new validators in the database
			validator_state.batch_store_validators(&validator_payload.validators).await?;
		}

		

		// TODO: Broadcast the new validators to the network
		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastValidators(validator_payload.clone()))
			.await
		{
			warn!("🏛 ⚠️  Consensus -  Broadcast Validators - Unable to write validators to network_client_tx channel: {:?}", e)
		}
		else
		{
			info!("🏛 Consensus -  Add New Validators - Successfully broadcasted validators for epoch {}", validator_payload.epoch);
		}
		Ok(())
	}

	pub async fn broadcast_new_block(&self, block_payload: BlockPayload) -> Result<(), Error> {
		if let Err(e) = self
			.network_client_tx
			.send(BroadcastNetwork::BroadcastValidateBlock(block_payload))
			.await
		{
			warn!("🏛 ⚠️  Consensus -  Broadcast New Block - Unable to write block to network_client_tx channel: {:?}", e)
		}
		Ok(())
	}

	pub async fn broadcast_vote(&self, vote: Vote) -> Result<(), Error> {

		debug!("🏛 Consensus -  Broadcast Vote - Broadcasting vote to network_client_tx channel, Block #: {:?}, Validator address: {:?}", vote.data.block_number, hex::encode(vote.validator_address));
		if let Err(e) = self.network_client_tx.send(BroadcastNetwork::BroadcastVote(vote)).await
		{
			warn!("🏛 ⚠️  Consensus -  Broadcast Vote - Unable to write vote to network_client_tx channel: {:?}", e)
		}
		Ok(())
	}

	pub async fn request_node_status(&self) -> Result<(), Error> {
		for peer_id in &self.real_time_checks.eligible_peers.1 {
			let peer_id = match PeerId::from_str(peer_id.as_str()) {
				Ok(peer_id) => peer_id,
				Err(e) => {
					error!("🏛 🚨  Consensus -  Request Node Status - Unable to calculate peer_id: {:?}", e);
					continue;
				}
			};

			debug!("🏛 Consensus -  Request Node Status - Broadcasting query node status request to peer: {:?}", peer_id.clone());

			if let Ok(request_time) = util::generic::current_timestamp_in_millis() {

				debug!("🏛 Consensus -  Request Node Status - request_time: {:?}", request_time.clone());
				
				if let Err(e) = self
					.network_client_tx
					.send(BroadcastNetwork::BroadcastQueryNodeStatusRequest(request_time, peer_id)).await
				{
					warn!("🏛 ⚠️  Consensus -  Request Node Status - Unable to write vote to network_client_tx channel: {:?}", e)
				}
			} else {
				error!("🏛 🚨  Consensus -  Request Node Status - Unable to get current timestamp");
			}

		}
		Ok(())
	}

	pub async fn handle_publish_node_detailed_status(&self,
		peer_id: String
	) -> Result<(), Error> {

		// Get Current Executed Block from Block Manager Cache
		let current_block  = {
			let block_manager_cache = BlockManagerCache::get_instance();
			match block_manager_cache.get_last_executed_block_header().await {
				Ok(block_header) => block_header.block_number,
				Err(e) => {
					warn!("🏛 ⚠️  Consensus -  Handle Publish Node Detailed Status - Failed to get current block: {:?}", e);
					0
				},
			}
		};

		// Get Mempool Size
		let mempool_size = {
			match self.get_mempool_size().await {
				Ok(size) => size,
				Err(e) => {
					warn!("🏛 ⚠️  Consensus -  Handle Publish Node Detailed Status - Failed to get mempool size: {:?}", e);
					0
				},
			}
		};

		// TODO: Patch this with Node Uptime
		let uptime_seconds = 0;

		// Get Connected Peers Count
		let connected_peers = {
			let network_state = NetworkState::get_instance();
			network_state.get_connected_peers_count().await
		};

		let node_detailed_status = NodeDetailedStatus {
			peer_id: peer_id.to_string(),
			current_block,
			pending_transactions: mempool_size as u32,
			uptime_seconds,
			connected_peers: connected_peers as u32,
		};

		match self.network_client_tx.send(BroadcastNetwork::BroadcastNodeDetailedStatus(node_detailed_status.clone())).await {
			Ok(_) => debug!("🏛 Consensus -  Handle Publish Node Detailed Status - Successfully sent node detailed status to network_client_tx channel, with peer_id: {:?}, node_detailed_status: {:?}", peer_id, node_detailed_status),
			Err(e) => warn!("🏛 ⚠️  Consensus -  Handle Publish Node Detailed Status - Failed to send node detailed status to peer: {:?}", e),
		};
		Ok(())
	}

	pub async fn handle_receive_node_detailed_status(&self, node_detailed_status: NodeDetailedStatus) -> Result<(), Error> {
		
		let network_state = NetworkState::get_instance();
		let last_update_time = util::generic::current_timestamp_in_millis().unwrap_or_default();
		let peer_id = PeerId::from_str(&node_detailed_status.peer_id).map_err(|e| anyhow!("Unable to get peer_id: {:?}", e))?;

		debug!("🏛 Consensus -  Handle Receive Node Detailed Status - Received node detailed status from peer: {:?}, node_detailed_status: {:?}", peer_id, node_detailed_status.clone());
		match network_state.update_active_peer_status_info(peer_id, PeerStatusInfo{
			current_block: Some(node_detailed_status.current_block),
			pending_transactions: Some(node_detailed_status.pending_transactions),
			connected_peers: Some(node_detailed_status.connected_peers),
			uptime_seconds: Some(node_detailed_status.uptime_seconds),
			last_update_time: Some(last_update_time as u64),
		}).await {
			Ok(_) => debug!("🏛 Consensus -  Handle Receive Node Detailed Status - Successfully updated node detailed status for peer: {:?}", peer_id),
			Err(e) => warn!("🏛 ⚠️  Consensus -  Handle Receive Node Detailed Status - Failed to update node detailed status for peer: {:?}", e),
		};

		Ok(())
	}

	// TODO: Implement this
	pub async fn get_mempool_size(&self) -> Result<usize, Error> {
		// let (sender, receiver) = oneshot::channel();
		// self.mempool_tx.send(ProcessMempool::GetSize(sender)).await.map_err(|e| anyhow!("Failed to send get mempool size request: {}", e))?;
	
		// receiver.await.map_err(|e| anyhow!("Failed to receive mempool size: {}", e))

		Ok(0)
	}
}

// pub async fn select_and_store_validators_and_proposer<'a>(epoch: Epoch,
// 													 last_block_header: &BlockHeader,
// 													 db_pool_conn: &'a DbTxConn<'a>,
// ) -> Result<(), Error> {
// 	let rt_config = RuntimeConfigCache::get().await?;

// 	// Select block validators
// 	let validator_manager = ValidatorManager{};
// 	let selected_validators = validator_manager
// 		.select_validators_for_epoch(
// 			&last_block_header,
// 			epoch,
// 			db_pool_conn
// 		)
// 		.await?;

// 	let validator_str = selected_validators.iter().map(|v| hex::encode(v.address)).collect::<Vec<String>>().join(", ");
// 	info!("🏛 Consensus -  Select Validators and Proposer - Selected Validators for Epoch: {}, Validators: {}",epoch,validator_str);

// 	// Store selected validators in validator state
// 	let validator_state = ValidatorState::new(db_pool_conn).await?;
// 	validator_state.batch_store_validators(&selected_validators).await?;
// 	// Select block proposer
// 	let mut block_proposer_manager = BlockProposerManager{};
// 	let mut eligible_block_proposers: Vec<Validator> = vec![];
// 	// filter out validator based on whitelisted/blacklisted nodes
// 	if let Some(whitelisted_block_proposers) = &rt_config.whitelisted_block_proposers {
// 		eligible_block_proposers = selected_validators.into_iter()
// 			.filter(|v| (whitelisted_block_proposers.contains(&v.address))).collect();
// 	} else if let Some(blacklisted_block_proposers) = &rt_config.blacklisted_block_proposers {
// 		eligible_block_proposers = selected_validators.into_iter()
// 			.filter(|v| (!blacklisted_block_proposers.contains(&v.address))).collect();
// 	}
	
// 	let block_proposer = block_proposer_manager.select_block_proposers(epoch, last_block_header, eligible_block_proposers, db_pool_conn).await?;
// 	info!("🏛 Consensus -  Select Validators and Proposer - Selected Block Proposer for Epoch: {}, Block Proposer: {}", epoch, hex::encode(block_proposer));
// 	Ok(())
// }
