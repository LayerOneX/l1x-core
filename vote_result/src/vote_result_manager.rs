use std::collections::HashMap;

use account::account_state::AccountState;
use anyhow::{anyhow, Error, Result};
use log::{info, warn};
use primitives::*;
use secp256k1::{hashes::sha256, Message, PublicKey, SecretKey};
use staking::staking_state::StakingState;
use system::{network::BroadcastNetwork, validator::Validator, vote::Vote, vote_result::VoteResult};
use db::db::DbTxConn;
use system::vote_result::VoteResultSignPayload;
use tokio::sync::mpsc;

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
			let min_votes = 1;

			// Check if the number of votes is less than the required minimum
			// We assume 30% of validators will be unresponsive or not available
			if votes.len() < min_votes {
				return Ok(None) //Err(anyhow!("Number of votes is less than 70% of validators"));
			}
			let total_favoured_stake = futures::future::join_all(votes.iter().map(
				|(validator_address, &vote)| async move {
					if vote {
						let staking_state = StakingState::new(db_pool_conn)
							.await
							.expect("error getting staking state conn");
						let stake_result = staking_state
							.get_staking_account(validator_address, pool_address)
							.await;

						match stake_result {
							Ok(stake) => {
								println!("Stake result: {}", stake);
								stake.balance as f64
							},
							Err(err) => {
								warn!("Error fetching stake: {:?}", err);
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

			// println!("TOTAL FAVOURED STAKE: {:?}", total_favoured_stake);
			voting_string.push_str(&format!("\tTotal favoured stake: {}\n", total_favoured_stake));

			// If 50% of the vote is in favour of the block, the block is accepted
			// let vote_passed = (total_favoured_stake / pool_balance as f64) > 0.5;
			let vote_passed = total_favoured_stake > 0.0;
			// info!("VOTE PASSED for block #{}? {:?}", block_number, vote_passed);
			voting_string.push_str(&format!("\t❔VOTE PASSED? {}\n", vote_passed));

			info!("{}", voting_string);

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

			return Ok(Some(vote_result))
		} else {
			return Err(anyhow!("No votes found for the block_hash"))
		}
	}

	pub fn try_to_generate_vote_result(
		block_number: BlockNumber,
		block_hash: BlockHash,
		cluster_address: Address,
		all_votes: Vec<Vote>,
		secret_key: &SecretKey,
		verifying_key: &PublicKey,
		block_proposer_address: Address,
		validators: Vec<Validator>,
	) -> Result<Option<VoteResult>, Error> {
		let mut validator_print = String::new();
		for v in validators.clone() { 
			let s = format!("\t{}\n", hex::encode(v.address));
			validator_print.push_str(&s);
		}
		// println!("SELECTED {} VALIDATORS for block {}: \n{}", selected_validators.len(),
		// block_number, validator_print);
		info!("VALIDATORS LOADED for block #{}: \n{}", block_number, validator_print);
		
		let unique_votes = Self::get_unique_votes(&all_votes);
		if !unique_votes.is_empty() {
			// If 50% of the vote is in favour of the block, the block is accepted
			// let vote_passed = (total_favoured_stake / pool_balance as f64) > 0.5;
			let min_votes = (validators.len() as f64 * 0.6).round() as usize;
			// Check if the number of votes is less than the required minimum
			// We assume 30% of validators will be unresponsive or not available
			info!("Min vote required #{}: total votes received{}", min_votes, unique_votes.len());
			if unique_votes.len() < min_votes {
				return Ok(None)
			}

			let vote_passed = Self::is_passed(&unique_votes, validators, min_votes);

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

			return Ok(Some(vote_result))
		} else {
			return Err(anyhow!("No votes found for the block_hash"))
		}
	}

	fn get_unique_votes(votes: &Vec<Vote>) -> Vec<Vote> {
		let mut unique_votes = HashMap::with_capacity(votes.len());
		votes.iter().for_each(|vote| {
			unique_votes.insert(&vote.verifying_key, vote);
		});
		
		unique_votes.into_iter().map(|(_, vote)| vote.clone()).collect::<Vec<Vote>>()
	}

	fn is_passed(all_votes: &Vec<Vote>, validators: Vec<Validator>, min_passed_votes: usize) -> bool {
		let mut address_validators = Vec::new();
		for validator in validators {
			address_validators.push(validator.address);
		};
		let passed_votes_count = all_votes
			.iter()
			.filter(|vote| address_validators.contains(&vote.validator_address) && vote.data.vote)
			.count();

		passed_votes_count >= min_passed_votes
	}

	pub fn is_vote_result_passed(vote_result: &VoteResult, validators: Vec<Validator>) -> bool {
		let unique_votes = Self::get_unique_votes(&vote_result.data.votes);
		let min_passed_votes = (validators.len() as f64 * 0.6) as usize;
		Self::is_passed(&unique_votes, validators, min_passed_votes)
	}
}
