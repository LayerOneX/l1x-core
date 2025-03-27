use compile_time_config::*;
use libp2p::PeerId;
use p2p::network::NetworkState;
use runtime_config::{RuntimeConfigCache, RuntimeStakingInfoCache};
use anyhow::Error;

use db::db::DbTxConn;
use log::{debug, info, warn};
use primitives::{Address, Epoch};
use system::{block_header::BlockHeader, config::Config, node_health::NodeHealth, validator::Validator};
use xscore::{kin_score::NodePerformanceMetrics, XScoreCalculator, stake_score::{NodeStakeInfo, StakeScoreCalculator}, XScoreWeights};
use block::block_manager::BlockManager;
use l1x_node_health::NodeHealthState;
use node_info::node_info_state::NodeInfoState;
use xscore::kin_score::KinScoreCalculator;
use rand::{rngs::StdRng, SeedableRng, seq::SliceRandom};
use sha2::{Sha256, Digest};
use std::str::FromStr;

pub struct ValidatorManager;

impl<'a> ValidatorManager {
	pub async fn select_validators_for_epoch(
		&self,
		last_block_header: &BlockHeader,
		epoch: Epoch,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<Vec<Validator>, Error> {
		let rt_config = RuntimeConfigCache::get().await?;
		let block_manager = BlockManager{};
		let node_info_state = NodeInfoState::new(db_pool_conn).await?;
		let node_health_state = NodeHealthState::new(&db_pool_conn).await?;
		let current_epoch = block_manager.calculate_current_epoch(last_block_header.block_number)?;
		let mut eligible_validators = Vec::new();

		let stacking_info = RuntimeStakingInfoCache::get().await?;
		
		
		for (node_address, info) in &stacking_info.nodes {

			debug!("select_validators_for_epoch ~ Node address: {:?}, Staked balance: {:?}, Min pool balance: {:?}", hex::encode(node_address), info.staked_balance, info.min_pool_balance);
			debug!("select_validators_for_epoch ~ Is org node: {:?}", rt_config.org_nodes.contains(node_address));
			debug!("select_validators_for_epoch ~ Staked balance less than min pool balance: {:?}", info.staked_balance < info.min_pool_balance);

			// Skip org nodes(always have the highest xscore) and nodes with staked balance less than the minimum balance
			if rt_config.org_nodes.contains(node_address) || info.staked_balance  < info.min_pool_balance {
				continue;
			}

			// Load node info
			let node_info = match node_info_state.load_node_info(node_address).await {
				Ok(info) => info,
				Err(e) => {
					log::warn!("Failed to load node info for 0x{} because {}. Skipping this validator.",
							   hex::encode(node_address), e);
					continue;
				}
			};

			debug!("select_validators_for_epoch ~ Node info: {:?}", node_info);
			// Skip this account if it doesn't have a peer_id
			if node_info.peer_id.is_empty() {
				log::warn!("Skipping validator selection for address 0x{} due to missing peer_id. Please update node info", hex::encode(node_address));
				continue;
			}

			// Load node health
			let node_health = match node_health_state
				.load_node_health(&node_info.peer_id, current_epoch)
				.await? {
				Some(health) => health,
				None => {
					log::warn!("Skipping validator selection for address 0x{} due to missing node health for epoch: {}", hex::encode(node_address), current_epoch);
					continue;
				},
			};

			debug!("select_validators_for_epoch ~ Node health: {:?}", node_health);

			// Calculate xscore
			let xscore = self
				.calculate_xscore(node_address, last_block_header, &node_health, &db_pool_conn)
				.await
				.map_err(|e| {
					log::error!("Error calculating xscore: {}", e);
					e
				})?;
				
			info!("Required XScore: {:?}, Node XScore: {:?}, Node Address:  {:?}", rt_config.xscore.xscore_threshold.clone(), xscore, hex::encode(node_address));

			// Get last executed block for the node
			let last_executed_block = {
				let network_state = NetworkState::get_instance();
				let peer_id = match PeerId::from_str(&node_info.peer_id) {
					Ok(peer_id) => peer_id,
					Err(e) => {
						log::error!("Error converting peer id to PeerId: {}", e);
						continue;
					}
				};
				let peer_status = match network_state.get_peer_status_info(peer_id).await {
					Some(peer_status) => peer_status,
					None => {
						log::error!("Error getting peer status info");
						continue;
					}
				};
				peer_status.current_block
			};

			let is_synced_node_synced = match last_executed_block {
				Some(height) => {
					let block_diff = last_block_header.block_number.saturating_sub(height);
					if block_diff <= MAX_BLOCK_DIFF.into() {
						debug!("Validator 0x{} is within sync range ({}  blocks behind).",
							hex::encode(node_address), block_diff);
						true
					} else {
						warn!("Skipping validator 0x{} as it's {} blocks behind (max allowed: {}).",
							hex::encode(node_address), block_diff, MAX_BLOCK_DIFF);
						false
					}
				},
				None => {
					// Keep validators we don't have block height for
					warn!("Cannot determine block height for validator 0x{}.",
						hex::encode(node_address));
					false
				}
			};

			if !is_synced_node_synced {
				continue;
			}

			eligible_validators.push(Validator {
				address: *node_address,
				cluster_address: last_block_header.cluster_address,
				epoch,
				stake: info.staked_balance,
				xscore,
			});
		}

		// Filter nodes:
		// 1. By XScore
		// 2. Node should be whitelisted or not blacklisted
		let mut selected_external_validators: Vec<Validator> = eligible_validators.into_iter()
			.filter(|v| v.xscore > rt_config.xscore.xscore_threshold)
			.collect();
		// filter out validator based on whitelisted/blacklisted nodes
		if let Some(whitelisted_nodes) = &rt_config.whitelisted_nodes {
			selected_external_validators = selected_external_validators.into_iter().filter(|v| whitelisted_nodes.contains(&v.address)).collect();
		} else if let Some(blacklisted_nodes) = &rt_config.blacklisted_nodes {
			selected_external_validators = selected_external_validators.into_iter().filter(|v| !blacklisted_nodes.contains(&v.address)).collect();
		}

		selected_external_validators.sort_by(|a, b| a.address.cmp(&b.address));

		
	
		// Take Max Validators from selected validators
		let mut org_validators: Vec<Validator> = vec![];

		// Add org nodes to the selected validators
		for org_node in rt_config.org_nodes.iter() {
			org_validators.push(Validator {
				address: *org_node,
				cluster_address: last_block_header.cluster_address,
				epoch,
				stake: 0,
				xscore: 1.0,
			});
		}

		debug!("Final selected external validators: Count: {:?}, List: {}", selected_external_validators.len(), selected_external_validators.iter().map(|v| hex::encode(v.address)).collect::<Vec<_>>().join(", "));
		debug!("Final selected fallback org validators: Count: {:?}, List: {}", org_validators.len(), org_validators.iter().map(|v| hex::encode(v.address)).collect::<Vec<_>>().join(", "));
		
		// Modified org node addition logic
		let max_validators = std::cmp::max(rt_config.max_validators, DEFAULT_MAX_VALIDATORS) as usize;

		// Calculate seed for shuffling
		let seed = self.calculate_seed(last_block_header.block_hash, epoch);
		let mut rng = StdRng::seed_from_u64(seed);

		// Shuffle the final selected validators and org nodes
		let (shuffled_validators, _) = selected_external_validators.partial_shuffle(&mut rng, max_validators as usize);
		let mut final_selected_validators = shuffled_validators.to_vec();
		// Add org nodes to the final selected validators
		final_selected_validators.extend(org_validators.clone());

		info!("Final selected validators: Org Fallback Validator: {:?}, Validator nodes: {:?}, List: {}", org_validators.len(), selected_external_validators.len(), final_selected_validators.iter().map(|v| hex::encode(v.address)).collect::<Vec<_>>().join(", "));
		Ok(final_selected_validators)
	}

	pub async fn calculate_xscore(&self, validator_address: &Address, last_block_header: &BlockHeader, node_health: &NodeHealth, db_pool_conn: &'a DbTxConn<'a>,) -> Result<f64, Error>{
		let staking_info = RuntimeStakingInfoCache::get().await?;
		let rt_config = RuntimeConfigCache::get().await?;

		let node_pool_info = staking_info.nodes
			.get(validator_address)
			.ok_or(anyhow::anyhow!("No staking info for '{}'", hex::encode(validator_address)))?;

		debug!("calculate_xscore ~ Calculating xscore for node: {:?}", hex::encode(validator_address));
		debug!("calculate_xscore ~ Node pool info: {:?}", node_pool_info);

		let mut stake_score_calculator = StakeScoreCalculator::new(12, &rt_config.stake_score); // Use the default weights as per the 12-month period
		stake_score_calculator.min_balance = node_pool_info.min_pool_balance;
		stake_score_calculator.max_balance = node_pool_info.max_pool_balance;

		debug!("calculate_xscore ~ Stake score calculator: {:?}", stake_score_calculator);

		let xscore_calculator = XScoreCalculator {
			stake_score_calculator,
			kin_score_calculator: KinScoreCalculator::new(&rt_config.kin_score),
			weights: XScoreWeights::new(&rt_config.xscore),
			xscore_threshold: rt_config.xscore.xscore_threshold,
		};

		let pool_stake_age = if let Some(min_pool_balance_from) = node_pool_info.min_pool_balance_from {
			last_block_header.timestamp.checked_sub(
				util::generic::milliseconds_to_seconds(min_pool_balance_from) as u64
			)
			.unwrap_or(0)
		} else {
			0
		};

		debug!("calculate_xscore ~ Pool stake age: {:?}", pool_stake_age);

		let node_stake_info = NodeStakeInfo {
			stake_balance: node_pool_info.staked_balance,
			stake_age_days: util::generic::seconds_to_days(pool_stake_age),
			..Default::default()
		};

		//FIXME: Hardcoded security_measure_score with calculated values
		//let block_state = BlockState::new(&db_pool_conn).await?;
		//let transaction_count = block_state.get_transaction_count(&validator_address, None).await?;

		let node_performance = NodePerformanceMetrics{
			uptime: node_health.uptime_percentage, 
			participation_history: Default::default(),
			response_time_ms: node_health.response_time_ms,
			security_measure_score: Default::default(),
		};

		let xscore = match xscore_calculator.calculate_xscore(
				&node_stake_info,
				&node_performance,
				&rt_config.stake_score) {
			Ok(xscore) => xscore,
			Err(e) => {
				return Err(anyhow::anyhow!("Failed to calculate xscore: {}", e));
			}
		};

		debug!("calculate_xscore ~ Xscore: {:?} for node: {:?}", xscore, hex::encode(validator_address));

		Ok(xscore)
	}

	async fn blocks_to_days(&self, block_number: u128) -> Result<u64, Error> {
		let config = Config::default();
		let average_block_time_seconds: u128 = (config.block_time) as u128;
	
		// Calculate total seconds as u128 to avoid precision loss
		let total_seconds = block_number.checked_mul(average_block_time_seconds)
			.ok_or_else(|| anyhow::anyhow!("Multiplication overflow"))?;
	
		// Calculate total days from seconds using safe arithmetic
		let seconds_in_a_day = 60_u128.checked_mul(60)
			.and_then(|result| result.checked_mul(24))
			.ok_or_else(|| anyhow::anyhow!("Multiplication overflow for seconds_in_a_day"))?;
	
		let total_days = total_seconds.checked_div(seconds_in_a_day)
			.ok_or_else(|| anyhow::anyhow!("Division overflow"))?;
	
		Ok(total_days as u64)
	}

	async fn calculate_stake_age(&self, stake_period: u128, stake_created_on: u128) -> Result<u64, Error> {
		// Ensure stake_period is not earlier than stake_created_on
		let block_count_since_stake_creation = stake_period.checked_sub(stake_created_on)
			.ok_or_else(|| anyhow::anyhow!("Stake period overflow"))?;
		
		// Convert block count to days using the blocks_to_days method
		self.blocks_to_days(block_count_since_stake_creation).await
	}

	pub fn calculate_seed(&self, last_block_hash: [u8; 32], current_epoch: u64) -> u64 {
		let mut hasher = Sha256::new();
		hasher.update(last_block_hash);
		hasher.update(current_epoch.to_be_bytes());
		let result = hasher.finalize();

		// Convert the first 8 bytes of the hash to a u64
		u64::from_be_bytes(result[0..8].try_into().unwrap())
	}
}