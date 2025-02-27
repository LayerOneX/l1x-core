use std::collections::HashMap;

use account::account_state::AccountState;
use anyhow::{anyhow, Error, Result};
use log::{info, warn, debug};
use primitives::*;
use secp256k1::{hashes::sha256, Message, PublicKey, SecretKey};
use staking::staking_state::StakingState;
use system::{network::BroadcastNetwork, validator::Validator, vote::Vote, vote_result::VoteResult};
use db::db::DbTxConn;
use system::vote_result::VoteResultSignPayload;
use tokio::sync::mpsc;
use std::collections::HashSet;
use runtime_config::{RuntimeConfigCache, RuntimeStakingInfoCache};
use std::sync::Arc;

const VOTE_THRESHOLD: f64 = 0.60; // 60%
const STAKE_PASS_NUMERATOR: u64 = 60;
const STAKE_PASS_DENOMINATOR: u64 = 100;

/// Returns the minimum number of votes required, based on a configurable threshold.
/// 
/// # Arguments
/// * `validators_count` - The total number of validators
/// * `threshold_percent` - The fraction (0.0 to 1.0) of validators needed
///
/// # Example
/// ```
/// let min_votes = calculate_min_votes(10, 0.7); // ~7
/// ```
fn calculate_min_votes(validators_count: usize, threshold_percent: f64) -> usize {
	// Add safety margin for floating point precision
	((validators_count as f64 * threshold_percent) - f64::EPSILON).ceil() as usize
}

pub struct VoteResultManager {
	pub network_client_tx: mpsc::Sender<BroadcastNetwork>,
	pub multinode_mode: bool,
}

impl<'a> VoteResultManager {
	pub fn new(
		network_client_tx: mpsc::Sender<BroadcastNetwork>,
		multinode_mode: bool,
	) -> VoteResultManager {
		VoteResultManager { network_client_tx, multinode_mode }
	}

	pub async fn vote_result(
		&self,
		block_number: BlockNumber,
		block_hash: &BlockHash,
		pool_address: &Address,
		validator_address: &Address,
		cluster_address: &Address,
		all_votes: Vec<Vote>,
		secret_key: &SecretKey,
		verifying_key: &PublicKey,
		validators: Vec<Validator>,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<Option<VoteResult>, Error> {
		let all_votes_hashmap: HashMap<Address, bool> = HashMap::from_iter(all_votes.iter().map(|vote| (vote.validator_address, vote.data.vote)));

		let pool_account = {
			let account_state = AccountState::new(db_pool_conn).await?;
			let pool_account = account_state.get_account(&pool_address).await?;
			pool_account
		};
		let pool_balance = pool_account.balance;
		debug!("vote_result ~ Pool balance: {:?}", pool_balance);

		// Validator Address hex encode
		debug!("vote_result ~ Validator address: {:?}", hex::encode(validator_address));

		let mut voting_string = format!("Vote info for block #{}\n", block_number);
		voting_string.push_str(&format!("\tPool balance: {}\n", pool_balance));

		// let all_votes_hashmap = {
		// 	let vote_state = VoteState::new(db_pool_conn).await?;
		// 	vote_state.load_all_votes_hashmap(&block_hash).await?
		// };

		if !all_votes_hashmap.is_empty() {
			let votes = all_votes_hashmap;
			// Calculate the minimum number of votes required (70% of validators)
			// let min_votes = (validators.len() as f64 * 0.7) as usize;
			let min_votes = calculate_min_votes(validators.len(), VOTE_THRESHOLD);
			debug!("vote_result ~ Min votes: {:?}", min_votes);
			debug!("vote_result ~ Votes length: {:?}", votes.len());
			debug!("vote_result ~ VOTE_THRESHOLD: {:?}", VOTE_THRESHOLD);


			// Check if the number of votes is less than the required minimum
			// We assume 30% of validators will be unresponsive or not available
			if votes.len() < min_votes {
				return Ok(None) //Err(anyhow!("Number of votes is less than 70% of validators"));
			}
			let total_favoured_stake = futures::future::join_all(votes.iter().map(
				|(validator_address, &vote)| async move {
					debug!("vote_result ~ total_favoured_stake ~ Validator address: {:?}, Vote: {:?}", hex::encode(validator_address), vote);
					if vote {
						let staking_state = StakingState::new(db_pool_conn)
							.await
							.expect("error getting staking state conn");
						let stake_result = staking_state
							.get_staking_account(validator_address, pool_address)
							.await;

						match stake_result {
							Ok(stake) => {
								debug!("vote_result ~ total_favoured_stake ~ Validator address: {:?}, Stake result: {}", hex::encode(validator_address), stake);
								stake.balance as f64
							},
							Err(err) => {
								warn!("vote_result ~ total_favoured_stake ~ Error fetching stake: {:?}, for validator address: {:?}, pool address: {:?}", err, hex::encode(validator_address), hex::encode(pool_address));
								0.0
							},
						}
					} else {
						0.0
					}
				},
			))
			.await
			.into_iter()
			.sum::<f64>();

			let total_voted_stake = futures::future::join_all(votes.iter().map(
				|(validator_address, _)| async move {
					let staking_state = StakingState::new(db_pool_conn)
						.await
						.expect("error getting staking state conn");
					debug!("vote_result ~ total_voted_stake ~ Validator address: {:?}", hex::encode(validator_address));
					match staking_state
						.get_staking_account(validator_address, pool_address)
						.await
					{
						Ok(stake) => stake.balance as f64,
						Err(err) => {
							warn!("vote_result ~ total_voted_stake ~ Error fetching stake: {:?}, for validator address: {:?}, pool address: {:?}", err, hex::encode(validator_address), hex::encode(pool_address));
							0.0
						},
					}
				},
			))
			.await
			.into_iter()
			.sum::<f64>();

			debug!("vote_result ~ total_voted_stake: {:?}", total_voted_stake);
			debug!("vote_result ~ total_favoured_stake: {:?}", total_favoured_stake);
			let stake_ratio = if total_voted_stake > 0.0 {
				total_favoured_stake / total_voted_stake
			} else {
				0.0
			};

			// println!("TOTAL FAVOURED STAKE: {:?}", total_favoured_stake);
			voting_string.push_str(&format!(
				"\tTotal voted stake: {}\n\tFavoured stake ratio: {:.2}\n",
				total_favoured_stake, stake_ratio
			));

			// If 50% of the vote is in favour of the block, the block is accepted
			// let vote_passed = (total_favoured_stake / pool_balance as f64) > 0.5;
			let vote_passed = stake_ratio > (STAKE_PASS_NUMERATOR as f64 / STAKE_PASS_DENOMINATOR as f64) - f64::EPSILON;
			debug!("vote_result ~ Vote passed: {:?} for block #{}, ratio: {}/{}", vote_passed, block_number, STAKE_PASS_NUMERATOR, STAKE_PASS_DENOMINATOR);
			// info!("VOTE PASSED for block #{}? {:?}", block_number, vote_passed);
			voting_string.push_str(&format!("\t❔VOTE PASSED? {}\n", vote_passed));

			info!("vote_result ~ voting_string: {}", voting_string);

			let vote_result_sign_payload = VoteResultSignPayload::new(
				block_number,
				*block_hash,
				*cluster_address,
				vote_passed,
				all_votes.clone(),
			);

			let json_str = serde_json::to_string(&vote_result_sign_payload).map_err(|e| {
				anyhow!(format!("Failed to serialize the vote signature payload: {:?}", e))
			})?;
			let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());
			let sig = secret_key.sign_ecdsa(message);

			let vote_result = VoteResult::new(
				block_number,
				*block_hash,
				*cluster_address,
				*validator_address,
				sig.serialize_compact().to_vec(),
				verifying_key.serialize().to_vec(),
				vote_passed,
				all_votes,
			);

			debug!("vote_result ~ Vote result: {:?}", vote_result);
			// Iterate Validator Address to hex::encode
			for v in vote_result.clone().data.votes {
				debug!("vote_result ~ Vote result ~ Validator address: {:?}", hex::encode(v.validator_address));
			}

			// {
			// 	let vote_result_state = VoteResultState::new(db_pool_conn).await?;
			// 	vote_result_state.store_vote_result(&vote_result).await?;
			// }
			if self.multinode_mode {
				if let Err(e) = self
					.network_client_tx
					.send(BroadcastNetwork::BroadcastVoteResult(vote_result.clone()))
					.await
				{
					warn!("Unable to write vote result too network_client_tx channel: {:?}", e)
				}
			}

			debug!(
				"Participation: {}/{} ({}%) required, {}/{} ({}%) approved",
				votes.len(),
				validators.len(),
				(votes.len() as f64 / validators.len() as f64) * 100.0,
				total_favoured_stake,
				total_voted_stake,
				stake_ratio * 100.0
			);

			return Ok(Some(vote_result))
		} else {
			return Err(anyhow!("No votes found for the block_hash"))
		}
	}

	pub async fn try_to_generate_vote_result(
		block_number: BlockNumber,
		block_hash: BlockHash,
		cluster_address: Address,
		all_votes: Vec<Vote>,
		secret_key: &SecretKey,
		verifying_key: &PublicKey,
		block_proposer_address: Address,
		validators: Vec<Validator>,
		db_pool_conn: &'a DbTxConn<'a>,
		pool_address: &Address,
	) -> Result<Option<VoteResult>, Error> {
		let mut validator_print = String::new();
		for v in validators.clone() { 
			let s = format!("\t{}\n", hex::encode(v.address));
			debug!("try_to_generate_vote_result ~ Validator Address: {:?}", s);
			validator_print.push_str(&s);
		}
		// println!("SELECTED {} VALIDATORS for block {}: \n{}", selected_validators.len(),
		// block_number, validator_print);
		info!("VALIDATORS LOADED for block #{}: \n{}", block_number, validator_print);
		debug!("try_to_generate_vote_result ~ pool_address: {:?}", hex::encode(pool_address));
		
		let unique_votes = Self::get_unique_votes(&all_votes);
		if !unique_votes.is_empty() {
			// If 50% of the vote is in favour of the block, the block is accepted
			// let vote_passed = (total_favoured_stake / pool_balance as f64) > 0.5;
			let min_votes = calculate_min_votes(validators.len(), VOTE_THRESHOLD);
			// Check if the number of votes is less than the required minimum
			// We assume 30% of validators will be unresponsive or not available
			info!("try_to_generate_vote_result ~ Min vote required #{}: total votes received #{}:", min_votes, unique_votes.len());
			if unique_votes.len() < min_votes {
				return Ok(None)
			}

			let vote_passed = Self::is_passed(&unique_votes, validators, min_votes).await?;
			debug!("try_to_generate_vote_result ~ Vote passed: {:?} for block #{}", vote_passed, block_number);
			// Waiting for 60% up votes for this block
			if !vote_passed {
				return Ok(None)
			}

			let mut voting_string = format!("Vote info for block #{}\n", block_number);
			// voting_string.push_str(&format!("\tTotal favoured stake: {}\n", total_favoured_stake));

			// info!("VOTE PASSED for block #{}? {:?}", block_number, vote_passed);
			voting_string.push_str(&format!("\t❔VOTE PASSED? {}\n", vote_passed));

			info!("{}", voting_string);

			let vote_result_sign_payload = VoteResultSignPayload::new(
				block_number,
				block_hash.clone(),
				cluster_address.clone(),
				vote_passed,
				unique_votes.clone(),
			);

			let json_str = serde_json::to_string(&vote_result_sign_payload).map_err(|e| {
				anyhow!(format!("Failed to serialize the vote signature payload: {:?}", e))
			})?;
			let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());
			let sig = secret_key.sign_ecdsa(message);

			let vote_result = VoteResult::new(
				block_number,
				block_hash,
				cluster_address,
				block_proposer_address,
				sig.serialize_compact().to_vec(),
				verifying_key.serialize().to_vec(),
				vote_passed,
				unique_votes,
			);


			debug!("try_to_generate_vote_result ~ Vote result: {:?}", vote_result);
			// Iterate Validator Address to hex::encode
			for v in vote_result.clone().data.votes {
				debug!("try_to_generate_vote_result ~ Vote result ~ Validator address: {:?}", hex::encode(v.validator_address));
			}

			return Ok(Some(vote_result))
		} else {
			return Err(anyhow!("No votes found for the block_hash"))
		}
	}

	fn get_unique_votes(votes: &Vec<Vote>) -> Vec<Vote> {
		let mut unique_votes = HashMap::new();
		votes.iter().for_each(|vote| {
			// Use both address and key for collision protection
			let key = (vote.validator_address, &vote.verifying_key[..8]);
			unique_votes.entry(key).or_insert(vote.clone());
		});
		unique_votes.into_values().collect()
	}

	// fn is_passed(all_votes: &Vec<Vote>, validators: Vec<Validator>, min_passed_votes: usize) -> bool {
	// 	let mut address_validators = Vec::new();
	// 	for validator in validators {
	// 		address_validators.push(validator.address);
	// 	};
	// 	let passed_votes_count = all_votes
	// 		.iter()
	// 		.filter(|vote| address_validators.contains(&vote.validator_address) && vote.data.vote)
	// 		.count();

	// 	passed_votes_count >= min_passed_votes
	// }

	async fn is_passed(
		all_votes: &Vec<Vote>, 
		validators: Vec<Validator>,
		min_participation_count: usize
	) -> Result<bool, Error> {
		// Get block number from votes (all votes should be for same block)
		let block_number = all_votes.first()
			.map(|v| v.data.block_number)
			.unwrap_or(0); // Default to genesis if empty (should never happen)

		debug!(
			"Vote evaluation for block #{}\n\
			\tTotal validators: {}\n\
			\tMinimum participation: {}\n\
			\tUnique votes received: {}",
			block_number,
			validators.len(),
			min_participation_count,
			all_votes.len()
		);

		// Get runtime config and stacking info for all nodes
		let rt_config:Arc<RuntimeConfigCache> = RuntimeConfigCache::get().await?;
		let staking_info:Arc<RuntimeStakingInfoCache> = RuntimeStakingInfoCache::get().await?;
		let min_stake_amount = rt_config.stake_score.min_balance;

		// 1. Filter to only votes from valid validators
		let validator_addresses: HashSet<Address> =
			validators.iter().map(|v| v.address).collect();
	
		let valid_votes: Vec<&Vote> = all_votes
			.iter()
			.filter(|vote| validator_addresses.contains(&vote.validator_address))
			.collect();
	
		// 2. Check if we have enough votes by count (participation threshold)
		if valid_votes.len() < min_participation_count {
			return Ok(false);
		}
	
		// Enhanced stake retrieval with multiple fallback mechanisms
		let get_validator_stake = |validator_address: &Address| -> f64 {
			debug!("is_passed ~ Resolving stake for {}", hex::encode(validator_address));
			
			// Org nodes always get minimum stake
			if rt_config.org_nodes.contains(validator_address) {
				debug!("is_passed ~ Org node with minimum stake: {}", min_stake_amount);
				return min_stake_amount as f64;
			}
			
			// For non-org nodes, check actual stake balance
			match staking_info.nodes.get(validator_address) {
				Some(stake) if stake.staked_balance >= min_stake_amount => {
					debug!("is_passed ~ Valid stake: {} for node", stake.staked_balance);
					stake.staked_balance as f64
				},
				Some(stake) => {
					debug!("is_passed ~ Stake below minimum: {} < {}", stake.staked_balance, min_stake_amount);
					0.0  // Exclude nodes with insufficient stake
				},
				None => {
					debug!("is_passed ~ No stake found for non-org node");
					0.0  // Exclude nodes with no stake info
				}
			}
		};
	
		let (total_voted_stake, total_yes_stake) = valid_votes.iter().fold(
			(0u128, 0u128),
			|(mut total, mut yes), vote| {
				// Detect org nodes using validator list markers
				let is_org_node = validators.iter()
					.find(|v| v.address == vote.validator_address)
					.map(|v| v.stake == 0 && (v.xscore - 1.0).abs() < f64::EPSILON)
					.unwrap_or(false);

				let stake = if is_org_node {
					// Use configured minimum balance for org nodes
					rt_config.stake_score.min_balance as u128
				} else {
					// Regular validator stake
					staking_info.nodes.get(&vote.validator_address)
						.map(|s| s.staked_balance)
						.unwrap_or(0)
				};

				total += stake;
				if vote.data.vote {
					yes += stake;
				}
				(total, yes)
			},
		);

		// Integer-based threshold check
		let passed = if total_voted_stake == 0 {
			false
		} else {
			total_yes_stake.checked_mul(STAKE_PASS_DENOMINATOR as u128)
				.and_then(|y| total_voted_stake.checked_mul(STAKE_PASS_NUMERATOR as u128)
					.map(|t| y >= t))
				.unwrap_or(false)
		};

		Ok(passed)
	}

	pub async fn is_vote_result_passed(
		vote_result: &VoteResult, 
		validators: Vec<Validator>
	) -> Result<bool, Error> {
		let unique_votes = Self::get_unique_votes(&vote_result.data.votes);
		let min_passed_votes = (validators.len() as f64 * VOTE_THRESHOLD) as usize;
		Self::is_passed(&unique_votes, validators, min_passed_votes).await
	}
}
