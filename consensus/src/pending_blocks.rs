use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Error};
use block::{block_state::BlockState, block_manager::BlockManager};
use db::db::DbTxConn;
use execute::execute_block::ExecuteBlock;
use log::{error, info, warn, debug};
use primitives::*;
use secp256k1::{PublicKey, SecretKey};
use system::{
	block::{ BlockPayload, QueryBlockMessage }, mempool::ProcessMempool, network::EventBroadcast, validator::Validator,
	vote::Vote, vote_result::VoteResult, config::Config
};
use tokio::sync::{broadcast, mpsc};
use block_proposer::block_proposer_manager::BlockProposerManager;
use l1x_vrf::common::SecpVRF;
use system::account::Account;
use system::block_proposer::BlockProposer;
use system::network::BroadcastNetwork;
use system::vote::VoteSignPayload;
use validate::{
	validate_block::ValidateBlock, validate_vote::ValidateVote,
	validate_vote_result::ValidateVoteResult,
};
use vote_result::vote_result_manager;
use vote_result::vote_result_state::VoteResultState;
use validator::validator_state::ValidatorState;
use node_info::node_info_state::NodeInfoState;
use libp2p::PeerId;
use crate::consensus::select_and_store_validators_and_proposer;
use runtime_config::RuntimeConfigCache;
use p2p::network::NetworkState;
use std::time::{SystemTime, UNIX_EPOCH};
// use primitives::constants::{VOTE_THRESHOLD, STAKE_PASS_NUMERATOR, STAKE_PASS_DENOMINATOR};

const QUERY_BLOCK_THRESHOLD: usize = 3;

#[derive(Debug, Clone)]
pub struct PendingBlock {
	votes: Vec<Vote>,
	vote_results: Vec<VoteResult>,
	block: Option<BlockPayload>,
	node_address: Address,
	pool_address: Address,
	secret_key: SecretKey,
	verifying_key: PublicKey,
	cluster_address: Address,
	mempool_tx: mpsc::Sender<ProcessMempool>,
	event_tx: broadcast::Sender<EventBroadcast>,
	node_event_tx: broadcast::Sender<EventData>,
	network_client_tx: mpsc::Sender<BroadcastNetwork>,
	network_state: Arc<NetworkState>,
	timestamp: u128,
}

impl<'a> PendingBlock {
	pub fn new(
		cluster_address: Address,
		node_address: Address,
		pool_address: Address,
		secret_key: SecretKey,
		verifying_key: PublicKey,
		mempool_tx: mpsc::Sender<ProcessMempool>,
		event_tx: broadcast::Sender<EventBroadcast>,
		node_event_tx: broadcast::Sender<EventData>,
		network_client_tx: mpsc::Sender<BroadcastNetwork>,
		network_state: Arc<NetworkState>,
	) -> Self {
		let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();

		Self {
			votes: Vec::new(),
			vote_results: Vec::new(),
			block: None,
			cluster_address,
			node_address,
			pool_address,
			secret_key,
			verifying_key,
			mempool_tx,
			event_tx,
			node_event_tx,
			network_client_tx,
			network_state,
			timestamp
		}
	}
	
	pub fn get_timestamp(&self) -> u128 {
		self.timestamp
	}
	
	pub fn add_vote(&mut self, vote: Vote) {
		self.votes.push(vote)
	}

	pub fn add_vote_result(&mut self, vote_result: VoteResult) {
		self.vote_results.push(vote_result)
	}

	pub fn set_block(&mut self, block: BlockPayload) {
		self.block = Some(block);
		self.timestamp = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("Time went backwards")
			.as_millis();
	}

	pub fn get_block(&self) -> Option<&BlockPayload> {
		self.block.as_ref()
	}

	pub fn all_votes_hashmap(&self) -> HashMap<Address, bool> {
		HashMap::from_iter(self.votes.iter().map(|vote| (vote.validator_address, vote.data.vote)))
	}

	pub fn all_votes(&self) -> Vec<Vote> {
		self.votes.clone()
	}

	pub async fn validate_votes(&self, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		let block_payload = self.get_block().ok_or(anyhow!("Block is not set"))?;
		for vote in &self.votes {
			ValidateVote::validate_vote(vote, &block_payload.block, db_pool_conn).await?
		}

		Ok(())
	}

	pub async fn validate_vote_results(&self, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		let block_payload = self.get_block().ok_or(anyhow!("Block is not set"))?;
		for vote_result in &self.vote_results {
			ValidateVoteResult::validate_vote_result(vote_result, &block_payload.block, db_pool_conn).await?;
		}

		Ok(())
	}

	pub async fn try_to_validate_block(&self, db_pool_conn: &'a DbTxConn<'a>) -> Result<(), Error> {
		let block_payload = self.get_block().ok_or(anyhow!("Block is not set"))?;
		let rt_config = RuntimeConfigCache::get().await?;
		let last_block_votes_validators = self.get_last_block_votes_validators(db_pool_conn, rt_config).await?;

		ValidateBlock::validate_proposed_block(&block_payload, &db_pool_conn, self.cluster_address.clone(), last_block_votes_validators)
			.await?;

		Ok(())
	}

	pub async fn try_to_vote_result(&self,
		secret_key: &SecretKey,
		verifying_key: &PublicKey,
		block_proposer_address: Address,
		validators: Vec<Validator>,
		db_pool_conn: &'a DbTxConn<'a>,
		pool_address: Address,
	) -> Result<VoteResult, Error> {
		let votes = self.all_votes();

		// Extract block timestamp from the pending block
		let block_timestamp = if let Some(block) = self.get_block() {
			block.block.block_header.timestamp as u128
		} else {
			// Fallback to current time if block not available (should be rare)
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("Time went backwards")
				.as_millis()
		};

		// Iterate Votes to hex::encode
		for v in votes.clone() {
			debug!("🤝 Consensus | Block #{} | Vote | Validator: {} | Epoch: {}", v.data.block_number, hex::encode(v.validator_address), v.data.epoch);
		}
		// Iterate Validators to hex::encode
		
		if let Some(vote_) = votes.get(0) {
			let block_number = vote_.data.block_number;
			let block_hash = vote_.data.block_hash;
			let cluster_address = vote_.data.cluster_address;

			debug!("🤝 Consensus | Block #{} | Votes Received | Count: {}", block_number, votes.len());

			let result = vote_result_manager::VoteResultManager::try_to_generate_vote_result(
				block_number, block_hash, cluster_address, self.all_votes(), secret_key, verifying_key, block_proposer_address, validators, db_pool_conn, &pool_address).await?;
			if let Some(vote_result) = result {
				Ok(vote_result)
			} else {
				Err(anyhow!("🤝 Consensus | Block #{} | Vote Result Generation Failed | Insufficient Votes", block_number))
			}
		} else {
			let block_number = self.get_block().and_then(|b| Some(b.block.block_header.block_number));
			Err(anyhow!("🤝 Consensus | Block #{:?} | Vote Result Generation Failed | Block Not Found", block_number))
		}
	}

	pub async fn try_to_finalize(
		&mut self,
		db_pool_conn: &'a DbTxConn<'a>,
		pool_address: Address,
	) -> Result<Vec<EventData>, Error> {
		let block_payload = self.get_block().cloned().ok_or(anyhow!("Block is not set"))?;

		// Check if node has reached maximum block height
		let rt_config = RuntimeConfigCache::get().await?;
		if let Some(max_block_height) = &rt_config.max_block_height {
			if block_payload.block.block_header.block_number >= *max_block_height as BlockNumber {
				error!("Node has reached the maximum configured block height: {}. Please upgrade or modify the configuration to continue.", max_block_height);
				return Err(anyhow!("🤝 Consensus | Block #{} | Max Height Reached | Limit: {}", block_payload.block.block_header.block_number, max_block_height));
			}
		}
		let last_block_votes_validators = self.get_last_block_votes_validators(db_pool_conn, rt_config.clone()).await?;

		ValidateBlock::validate_proposed_block(&block_payload, &db_pool_conn, self.cluster_address.clone(), last_block_votes_validators)
			.await?;

		// select validators if not present
		let validator_state = ValidatorState::new(&db_pool_conn).await?;
		let mut validators = validator_state
			.load_all_validators(block_payload.block.block_header.epoch)
			.await?
			.ok_or(anyhow!("🤝 Consensus | Epoch {} | Validator Selection Failed | No Validators Found", block_payload.block.block_header.epoch))?;

		
		let mut added_org_nodes = 0;
		for org_node in rt_config.org_nodes.iter()
			// .filter(|n| self.network_state.is_peer_available(n))
			.filter(|n| !validators.iter().any(|v| &v.address == *n))
		{
			let backup_validator = Validator {
				address: *org_node,
				cluster_address: self.cluster_address,
				epoch: block_payload.block.block_header.epoch,
				stake: 0,
				xscore: 1.0,
			};
			validator_state.upsert_validator(&backup_validator).await?;
			added_org_nodes += 1;
		}

		if added_org_nodes > 0 {
			debug!("🤝 Consensus | Added Backup Validators | Count: {} | Type: Org Nodes", added_org_nodes);
			// Reload validators with new backups
			validators = validator_state
				.load_all_validators(block_payload.block.block_header.epoch)
				.await?
				.ok_or(anyhow!("🤝 Consensus | Epoch {} | Validator Selection Failed | No Validators After Backup Addition", block_payload.block.block_header.epoch))?;
		}

		
		let block_proposer_address = Account::address(&self.verifying_key.serialize().to_vec())?;

		let block_proposer_manager = BlockProposerManager {};
		let block_proposer = BlockProposer::new(
			block_payload.block.block_header.cluster_address,
			block_payload.block.block_header.epoch,
			block_proposer_address,
		);


		// publish vote if and only if node is a validator, not a block proposer
		let is_validator = validator_state.is_validator(&self.node_address, block_payload.block.block_header.epoch).await?;
		let is_block_proposer = block_proposer_manager.is_block_proposer(block_proposer, &db_pool_conn).await?;
		debug!("🤝 Consensus | Block #{} | Epoch: {} | Node: {} | Status: Validator: {} | Proposer: {}", 
			block_payload.block.block_header.block_number,
			block_payload.block.block_header.epoch, 
			hex::encode(self.node_address),
			is_validator,
			is_block_proposer);

		// if is_validator && !is_block_proposer
		if is_validator || is_block_proposer
		{
			let vote = if let Some(vote) = self.votes.iter().find(|vote| vote.verifying_key == self.verifying_key.serialize().to_vec()).cloned() {
				debug!("🤝 Consensus | Block #{} | Vote Found | Validator: {}", block_payload.block.block_header.block_number, hex::encode(vote.validator_address));
				vote
			} else {
				let valid_block = true;
				let vote_sign_payload = VoteSignPayload::new(
					block_payload.block.block_header.block_number,
					block_payload.block.block_header.block_hash,
					self.cluster_address.clone(),
					block_payload.block.block_header.epoch,
					valid_block
				);

				let sig = vote_sign_payload.sign_with_ecdsa(self.secret_key)?;

				let vote = Vote::new(
					vote_sign_payload,
					self.node_address.clone(),
					sig.serialize_compact().to_vec(),
					self.verifying_key.serialize().to_vec(),
				);

				debug!("🤝 Consensus | Block #{} | Vote Added | Validator: {}", block_payload.block.block_header.block_number, hex::encode(vote.validator_address));

				self.add_vote(vote.clone());
				vote
			};

			debug!("🤝 Consensus | Block #{} | Broadcasting Vote | Validator: {}", vote.data.block_number, hex::encode(vote.validator_address));
			if let Err(e) =
				self.network_client_tx.send(BroadcastNetwork::BroadcastVote(vote)).await
			{
				warn!("🤝 ⚠️ Consensus | Network Error | Failed to Broadcast Vote | Error: {}", e);
			}
		} else {
			warn!("🤝 ⚠️ Consensus | Block #{} | Seems I am Peer Node (Neither Validator nor Proposer)", block_payload.block.block_header.block_number);
		}

		self.validate_vote_results(db_pool_conn).await?;

		let mut passed_vote_result: Option<VoteResult> = None;
	
		for v in self.vote_results.iter() {
			// Extract block timestamp from the block payload
			let block_timestamp = block_payload.block.block_header.timestamp as u128;
			
			let passed = vote_result_manager::VoteResultManager::is_vote_result_passed_with_fallback(
				v,
				validators.clone(),
				block_timestamp
			)
			.await?;
			
			debug!("🤝 Consensus | Block #{} | Vote Result | Validator: {} | Passed: {}", v.data.block_number, hex::encode(v.validator_address), passed);
			
			if passed {
				passed_vote_result = Some(v.clone());
				break; // stop after finding the first passing vote
			}
		}

		if let Some(vote_result) = passed_vote_result {
			// calculating new block proposer
			let block_manager = BlockManager{};
			let block_number = block_payload.block.block_header.block_number + 1;
			let new_epoch = block_manager.calculate_current_epoch(block_number)?;
					
			if new_epoch > block_payload.block.block_header.epoch {
		
				debug!("🤝 Consensus | Block #{} | Epoch {} | End of Epoch | Proposer Selection", block_number, new_epoch);
				// let last_block_header = {
				// 	let block_state = BlockState::new(db_pool_conn).await?;
				// 	block_state.block_head_header(block_payload.block.block_header.cluster_address).await?
				// };
				select_and_store_validators_and_proposer(
					new_epoch,
					&block_payload.block.block_header,
					db_pool_conn
				).await.map_err(|error| anyhow!("🤝 Consensus | Block #{} | Epoch {} | Proposer Selection Failed | Error: {}", block_number, new_epoch, error))?;
			}

			// store vote_result
			let vote_state = VoteResultState::new(&db_pool_conn).await?;
			vote_state.store_vote_result(&vote_result).await?;

			debug!("🤝 Consensus | Block #{} | Vote Result Stored | Validator: {}", vote_result.data.block_number, hex::encode(vote_result.validator_address));

			// store block
			let block_state = BlockState::new(&db_pool_conn).await?;
			block_state.store_block(block_payload.block.clone()).await?;

			for tx in &block_payload.block.transactions {
				let _ = self.mempool_tx.send(ProcessMempool::RemoveTrasaction(tx.clone())).await;
			}
			let events = ExecuteBlock::execute_block(
				&block_payload.block,
				self.event_tx.clone(),
				db_pool_conn,
			)
			.await?;

			self.broadcast_events(events.clone());

			Ok(events)
		} else {
			Err(anyhow!("🤝 Consensus | Block #{} | Waiting for more votes", block_payload.block.block_header.block_number))
		}
	}

	fn broadcast_events(&self, events: Vec<EventData>) {
		for event in events {
			if let Err(err) = self.node_event_tx.send(event) {
				warn!("🤝 ⚠️ Consensus | Event Broadcast Failed | Error: {}", err);
			}
		}
	}

	async fn get_last_block_votes_validators(
		&self,
		db_pool_conn: &'a DbTxConn<'a>,
		rt_config: Arc<RuntimeConfigCache>
	) -> Result<HashSet<Address>, Error> {
		let block_state = BlockState::new(db_pool_conn).await?;
		let block_header = block_state.block_head_header(self.cluster_address.clone()).await?;
		let vote_result_state = VoteResultState::new(&db_pool_conn).await?;
		let mut last_block_votes_validators = HashSet::new();
		if let Ok(vote_result) = vote_result_state.load_vote_result(&block_header.block_hash).await {
			for vote in vote_result.data.votes {
				let address = Account::address(&vote.verifying_key)?;
				if !rt_config.org_nodes.contains(&address) {
					last_block_votes_validators.insert(address);
				}
			}
		}
		Ok(last_block_votes_validators)
	}
}

pub struct PendingBlocks {
	blocks: HashMap<BlockNumber, PendingBlock>,
	cluster_address: Address,
	node_address: Address,
	pool_address: Address,
	secret_key: SecretKey,
	verifying_key: PublicKey,
	mempool_tx: mpsc::Sender<ProcessMempool>,
	event_tx: broadcast::Sender<EventBroadcast>,
	node_event_tx: broadcast::Sender<EventData>,
	network_client_tx: mpsc::Sender<BroadcastNetwork>,
	network_state: &'static Arc<NetworkState>,
}

impl<'a> PendingBlocks {
	pub fn new(
		cluster_address: Address,
		node_address: Address,
		pool_address: Address,
		secret_key: SecretKey,
		verifying_key: PublicKey,
		mempool_tx: mpsc::Sender<ProcessMempool>,
		event_tx: broadcast::Sender<EventBroadcast>,
		node_event_tx: broadcast::Sender<EventData>,
		network_client_tx: mpsc::Sender<BroadcastNetwork>,
	) -> Self {
		Self { 
			blocks: HashMap::new(),
			cluster_address,
			node_address,
			pool_address,
			secret_key,
			verifying_key,
			mempool_tx, 
			event_tx, 
			node_event_tx,
			network_client_tx,
			network_state: NetworkState::get_instance(),
		}
	}

	fn get_new_pending_block(&self) -> PendingBlock {
		PendingBlock::new(
			self.cluster_address.clone(),
			self.node_address.clone(),
			self.pool_address.clone(),
			self.secret_key.clone(),
			self.verifying_key.clone(),
			self.mempool_tx.clone(),
			self.event_tx.clone(),
			self.node_event_tx.clone(),
			self.network_client_tx.clone(),
			self.network_state.clone(),
		)
	}

	pub fn add_vote(&mut self, vote: Vote) {
		let block_number = vote.data.block_number;
		if let Some(pending_block) = self.blocks.get_mut(&block_number) {
			pending_block.add_vote(vote);
		} else {
			let mut pending_block = self.get_new_pending_block();
			pending_block.add_vote(vote);
			self.blocks.insert(block_number, pending_block);
		}
	}

	pub fn add_vote_result(&mut self, vote_result: VoteResult) {
		let block_number = vote_result.data.block_number;
		if let Some(pending_block) = self.blocks.get_mut(&block_number) {
			pending_block.add_vote_result(vote_result);
		} else {
			let mut pending_block = self.get_new_pending_block();
			pending_block.add_vote_result(vote_result);
			self.blocks.insert(block_number, pending_block);
		}
	}

	pub fn add_block(&mut self, block_payload: BlockPayload) {
		let block_number = block_payload.block.block_header.block_number;
		let now = SystemTime::now()
						.duration_since(UNIX_EPOCH)
						.expect("Time went backwards")
						.as_millis();

		if let Some(pending_block) = self.blocks.get_mut(&block_number) {
			pending_block.set_block(block_payload);
			pending_block.timestamp = now;  // Update timestamp even for existing blocks
		} else {
			let mut pending_block = self.get_new_pending_block();
			pending_block.set_block(block_payload);
			pending_block.timestamp = now;  // Set initial timestamp
			self.blocks.insert(block_number, pending_block);
		}
	}

	pub fn all_votes(&self, block_number: BlockNumber) -> Vec<Vote> {
		if let Some(pending_block) = self.blocks.get(&block_number) {
			pending_block.all_votes()
		} else {
			Vec::new()
		}
	}

	pub async fn try_to_vote_result(
		&self,
		block_number: BlockNumber,
		secret_key: &SecretKey,
		verifying_key: &PublicKey,
		block_proposer_address: Address,
		validators: Vec<Validator>,
		db_pool_conn: &'a DbTxConn<'a>,
		pool_address: Address,
	) -> Result<VoteResult, Error> {
		if let Some(pending_block) = self.blocks.get(&block_number) {
			pending_block.try_to_vote_result(secret_key, verifying_key, block_proposer_address, validators, db_pool_conn, pool_address).await
		} else {
			Err(anyhow!("🤝 Consensus | Block #{} | Operation Failed | Pending Block Not Found", block_number))
		}
	}

	pub fn expire_old_blocks(&mut self, expiration_ms: u128) -> Vec<BlockNumber> {
		let current_time = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("Time went backwards")
			.as_millis();

		let mut expired = Vec::new();
		self.blocks.retain(|&block_number, pending_block| {
			// Handle potential time going backwards (NTP adjustments etc)
			let block_time = pending_block.get_timestamp();
			let elapsed = current_time.saturating_sub(block_time);
			let retain = elapsed <= expiration_ms;
			
			debug!("🤝 Consensus | Block #{} | Expiration Check | Timestamp: {} | Elapsed: {} | Retained: {}", block_number, block_time, elapsed, retain);
			if !retain {
				expired.push(block_number);
			}
			retain
		});
		expired
	}

	pub fn get_blocks(&self) -> &HashMap<BlockNumber, PendingBlock> {
		&self.blocks
	}

	pub fn remove_block(&mut self, block_number: BlockNumber) {
		self.blocks.remove(&block_number);
	}

	pub async fn try_to_validate_block(
		&self,
		block_number: BlockNumber,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		if let Some(pending_block) = self.blocks.get(&block_number) {
			pending_block.try_to_validate_block(db_pool_conn).await
		} else {
			Err(anyhow!("Can't find pending Block #{}", block_number))
		}
	}

	pub async fn try_to_finalize(&mut self, db_pool_conn: &'a DbTxConn<'a>, pool_address: Address) -> Result<(), Error> {
		if self.blocks.is_empty() {
			return Ok(());
		}

		let mut block_numbers = self.blocks.keys().cloned().collect::<Vec<_>>();
		block_numbers.sort();

		let print_block_numbers = if block_numbers.len() > 5 {
			format!("[{}..{}]", block_numbers.first().unwrap(), block_numbers.last().unwrap())
		} else {
			format!("{:?}", block_numbers)
		};

		info!("🤝 Consensus | Pending Blocks: {} | Range: {} | Status: Unfinalized", block_numbers.len(), print_block_numbers);

		let mut finalized_blocks = Vec::new();
		for block_number in block_numbers {
			let block_state = BlockState::new(&db_pool_conn).await?;
			match block_state.is_block_executed(block_number, &self.cluster_address).await {
				Ok(true) => {
					// if block is already executed then it's already finalized
					finalized_blocks.push(block_number);
					continue;
				},
				_ => {
					info!("🤝 Consensus | Block #{} | Starting Finalization", block_number);
					if let Some(pending_block) = self.blocks.get_mut(&block_number) {
						match pending_block.try_to_finalize(db_pool_conn, pool_address).await {
							Ok(_e) => {
								info!("🤝 ✅ Consensus | Block #{} | Finalization Complete", block_number);
								finalized_blocks.push(block_number)
							},
							Err(e) => {
								warn!("🤝 ⚠️ Consensus | Block #{} | Finalization Failed | Reason: {}", block_number, e);
								// Blocks are sorted, no sense to try to finalize the next one
								break;
							},
						}
					} else {
						info!("🤝 ⚠️ Consensus | Block #{} | Missing Pending Block", block_number);
						// Blocks are sorted, no sense to try to finalize the next one
						break;
					}
				},
			}
		}

		for block_number in &finalized_blocks {
			self.blocks.remove(block_number);
		}

		// query for missing block (if any)
		let unfinalized_blocks = self.blocks.keys().collect::<Vec<_>>();
		if unfinalized_blocks.len() > QUERY_BLOCK_THRESHOLD {
			query_missed_block(
				self.cluster_address,
				self.node_address,
				self.network_client_tx.clone(),
				db_pool_conn,
			).await?;
		}

		Ok(())
	}
}

async fn query_missed_block<'a>(
	cluster_address: Address,
	node_address: Address,
	network_client_tx: mpsc::Sender<BroadcastNetwork>,
	db_pool_conn: &'a DbTxConn<'a>,
) -> Result<(), Error> {
	let config: Config = Config::get_config().await?;
	let block_state = BlockState::new(&db_pool_conn).await?;
	let block_number = block_state
		.block_head_header(cluster_address)
		.await?
		.block_number
		.checked_add(1)
		.ok_or_else(|| anyhow!("🤝 Consensus | Block Number Overflow | Maximum Block Height Reached"))?;

		info!("🤝 🔄 Consensus | Block #{} | Querying Missing Block", block_number);

	// Get peer_id of the boot node(if present) else select one of the validators to get peer_id
	let peer_id = if !config.boot_nodes.is_empty() {
		let boot_node = config.boot_nodes.get(0)
			.ok_or_else(|| anyhow!("🤝 Consensus | Network Configuration Error | No Boot Nodes Available"))?;
		let peer_id_str = boot_node
			.split("/p2p/")
			.nth(1)
			.ok_or_else(|| anyhow!("🤝 Consensus | Network Configuration Error | Invalid Boot Node Peer ID"))?;

		PeerId::from_str(peer_id_str)
			.map_err(|e| anyhow!("Unable to get peer ID: {:?}", e))?
	} else {
		let validator_state = ValidatorState::new(&db_pool_conn).await?;
		let block_manager = BlockManager{};
		let epoch  =  block_manager.calculate_current_epoch(block_number)?;
		let validators = validator_state
			.load_all_validators(epoch)
			.await?
			.ok_or(anyhow!("No validators selected for this epoch"))?;

		let node_info_state = NodeInfoState::new(db_pool_conn).await?;

		// Iterate over validators to find one that is not the current node
		let mut peer_id: Option<PeerId> = None;
		for validator in validators {
			if validator.address == node_address {
				continue; // Skip if validator address matches the node's address
			}
			match node_info_state.load_node_info(&validator.address).await {
				Ok(info) => {
					if let Ok(pid) = PeerId::from_str(&info.peer_id) {
						peer_id = Some(pid);
						break; // Exit loop once a valid peer_id is found
					} else {
						warn!("🤝 ⚠️ Consensus | Validator: {} | Invalid Peer ID: {}", hex::encode(&validator.address), info.peer_id);
					}
				}
				Err(e) => {
					warn!("🤝 ⚠️ Consensus | Validator: {} | Node Info Load Failed | Error: {}", hex::encode(&validator.address), e);
				}
			}
		}

		peer_id.ok_or_else(|| anyhow!("🤝 Consensus | Network Error | No Valid Peer IDs Found Among Validators"))?
	};

	let query_block_request = QueryBlockMessage {
		block_number,
		cluster_address
	};

	if let Err(e) = network_client_tx
		.send(BroadcastNetwork::BroadcastQueryBlockRequest(query_block_request, peer_id))
		.await
	{
		warn!("🤝 ⚠️ Consensus | Network Error | Failed to Query Block | Error: {}", e);
	}

	Ok(())
}
