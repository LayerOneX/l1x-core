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
// use primitives::constants::{VOTE_THRESHOLD, STAKE_PASS_NUMERATOR, STAKE_PASS_DENOMINATOR, BLOCK_EXPIRATION_TIME};
use compile_time_config::voting_config::{VOTE_THRESHOLD, STAKE_PASS_NUMERATOR, STAKE_PASS_DENOMINATOR, BLOCK_EXPIRATION_TIME};
use std::time::{SystemTime, UNIX_EPOCH};

// const STAKE_PASS_NUMERATOR: u64 = 60;
// const STAKE_PASS_DENOMINATOR: u64 = 100;

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
		let block_number = all_votes.first()
		  .map(|v| v.data.block_number)
		  .ok_or_else(|| anyhow!("No votes provided"))?;
	
		// Get configs
		let rt_config = RuntimeConfigCache::get().await?;
		let staking_info = RuntimeStakingInfoCache::get().await?;
		let min_stake_amount = rt_config.stake_score.min_balance;
	
		// Create validator lookup
		let validator_addresses: HashSet<Address> = validators.iter().map(|v| v.address).collect();
	
		// Filter valid votes and calculate stakes
		let mut total_stake: u128 = 0;
		let mut yes_stake: u128 = 0;
		let mut no_stake: u128 = 0;
		let mut valid_vote_count = 0;
	
		for vote in all_votes.iter() {
		  if !validator_addresses.contains(&vote.validator_address) {
			debug!("Skipping vote from non-validator: {}", hex::encode(&vote.validator_address));
			continue;
		  }
	
		  valid_vote_count += 1;
	
		  // Get stake
		  let stake = if rt_config.org_nodes.contains(&vote.validator_address) {
			min_stake_amount as u128
		  } else {
			match staking_info.nodes.get(&vote.validator_address) {
			  Some(stake_info) if stake_info.staked_balance >= min_stake_amount => {
				stake_info.staked_balance
			  },
			  Some(stake_info) => {
				debug!("Validator {} has insufficient stake: {}", 
				  hex::encode(&vote.validator_address), stake_info.staked_balance);
				continue;
			  },
			  None => {
				debug!("No stake info found for validator {}", 
				  hex::encode(&vote.validator_address));
				continue;
			  }
			}
		  };
	
		  total_stake = total_stake.checked_add(stake)
			.ok_or_else(|| anyhow!("Stake overflow in total calculation"))?;
	
		  if vote.data.vote {
			yes_stake = yes_stake.checked_add(stake)
			  .ok_or_else(|| anyhow!("Stake overflow in yes calculation"))?;
		  } else {
			no_stake = no_stake.checked_add(stake)
			  .ok_or_else(|| anyhow!("Stake overflow in no calculation"))?;
		  }
		}
	
		// Check minimum participation by both count and stake
		if valid_vote_count < min_participation_count {
		  debug!("Insufficient vote count: {} < {}", valid_vote_count, min_participation_count);
		  return Ok(false);
		}
	
		let voted_stake = yes_stake.checked_add(no_stake)
		  .ok_or_else(|| anyhow!("Overflow adding yes and no stakes"))?;
	
		// Calculate required yes stake (60% of voted stake)
		let required_yes_stake = voted_stake
		  .checked_mul(STAKE_PASS_NUMERATOR as u128)
		  .and_then(|n| n.checked_div(STAKE_PASS_DENOMINATOR as u128))
		  .ok_or_else(|| anyhow!("Arithmetic overflow in threshold calculation"))?;
	
		debug!(
		  "Vote stake distribution for block #{}: \n\
		   Total valid votes: {}\n\
		   Yes stake: {}\n\
		   No stake: {}\n\
		   Required yes stake: {}\n\
		   Passed: {}",
		  block_number, valid_vote_count, yes_stake, no_stake, 
		  required_yes_stake, yes_stake >= required_yes_stake
		);
	
		Ok(yes_stake >= required_yes_stake)
	  }


	pub async fn is_vote_result_passed(
		vote_result: &VoteResult, 
		validators: Vec<Validator>
	) -> Result<bool, Error> {
		let unique_votes = Self::get_unique_votes(&vote_result.data.votes);
		let min_passed_votes = (validators.len() as f64 * VOTE_THRESHOLD) as usize;

		debug!("is_vote_result_passed ~ unique_votes: {:?}", unique_votes);
		debug!("is_vote_result_passed ~ min_passed_votes: {:?}", min_passed_votes);

		Self::is_passed(&unique_votes, validators, min_passed_votes).await
	}

	pub async fn is_vote_result_passed_with_fallback(
        vote_result: &VoteResult,
        validators: Vec<Validator>,
        block_timestamp: u128,
    ) -> Result<bool, Error> {
		let rt_config:Arc<RuntimeConfigCache> = RuntimeConfigCache::get().await?;
        // First pass: External validators only
        let external_validators: Vec<Validator> = validators.iter()
            .filter(|v| !rt_config.org_nodes.contains(&v.address))
            .cloned()
            .collect();

        if !external_validators.is_empty() {
            match Self::is_vote_result_passed(vote_result, external_validators).await {
                Ok(true) => return Ok(true),
                Err(e) => return Err(e),
                _ => {} // Continue to fallback
            }
        }

        // Fallback after 30 seconds
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| anyhow!("System time before UNIX EPOCH!"))?
            .as_millis();

        if current_time.saturating_sub(block_timestamp) > BLOCK_EXPIRATION_TIME {
            Self::is_vote_result_passed(vote_result, validators).await
        } else {
            Ok(false)
        }
    }
}