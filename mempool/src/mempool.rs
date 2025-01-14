use account::account_state::AccountState;
use anyhow::{anyhow, Error};
use block::{block_manager::BlockManager, block_state::BlockState};
use block_proposer::block_proposer_manager::BlockProposerManager;
use compile_time_config::SYSTEM_REWARDS_DISTRIBUTOR;
use db::db::DbTxConn;
use execute::execute_block::ExecuteBlock;
use log::{debug, info, warn};
use primitives::{Address, Balance, BlockHash, BlockNumber, EventData, MemPoolSize, TimeStamp};
use secp256k1::{Message, PublicKey, SecretKey};
use std::collections::{HashMap, HashSet};
use secp256k1::hashes::sha256;
use system::{
	account::Account, network::{BroadcastNetwork, EventBroadcast}, transaction::{Transaction, TransactionType}
};
use tokio::sync::{broadcast, mpsc, oneshot};
use execute::execute_fee;
use system::block::BlockPayload;
use system::network::{BlockProposerEventType, NetworkAcknowledgement, NetworkMessage};
use validate::validate_common::ValidateCommon;
use util::generic::{current_timestamp_in_millis, seconds_to_milliseconds};

pub enum ProposeBlockResult {
	Proposed(Vec<EventData>),
	NotProposed,
}

pub struct Mempool {
	pub transactions: HashMap<Address, HashMap<TimeStamp, Vec<Transaction>>>,
	pub transactions_priority: Vec<(Transaction, TimeStamp)>,
	pub max_size: MemPoolSize,
	pub fee_limit: Balance,
	pub rate_limit: usize,
	pub time_frame_seconds: TimeStamp,
	pub expiration_seconds: TimeStamp,
	pub network_client_tx: mpsc::Sender<BroadcastNetwork>,
	pub event_tx: broadcast::Sender<EventBroadcast>,
	pub cluster_address: Address,
	pub node_address: Address,
	pub secret_key: SecretKey,
	pub verifying_key: PublicKey,
	pub multinode_mode: bool,
}

impl<'a> Mempool {
	pub fn new(
		max_size: MemPoolSize,
		fee_limit: Balance,
		rate_limit: usize,
		time_frame_seconds: TimeStamp,
		expiration_seconds: TimeStamp,
		network_client_tx: mpsc::Sender<BroadcastNetwork>,
		event_tx: broadcast::Sender<EventBroadcast>,
		cluster_address: Address,
		node_address: Address,
		secret_key: SecretKey,
		verifying_key: PublicKey,
		multinode_mode: bool,
	) -> Self {
		Mempool {
			transactions: HashMap::new(),
			transactions_priority: Vec::new(),
			max_size,
			fee_limit,
			rate_limit,
			time_frame_seconds,
			expiration_seconds,
			network_client_tx,
			event_tx,
			cluster_address,
			node_address,
			secret_key,
			verifying_key,
			multinode_mode,
		}
	}

	pub async fn add_transaction(
		&mut self,
		mut transaction: Transaction,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> Result<(), Error> {
		// Check if the mempool is full
		if self.transactions_priority.len() >= self.max_size {
			return Err(anyhow!("Mempool is full"))
		}

		let transaction_hash = transaction.transaction_hash()?;
		let transaction_hash_string = hex::encode(&transaction_hash);

		// Check if the transaction fee is within the fee limit
		if transaction.fee_limit > self.fee_limit {
			return Err(anyhow!("Transaction ({}) fee exceeds fee limit: {} > {}", transaction_hash_string, transaction.fee_limit, self.fee_limit));
		}

		let sender = Account::address(&transaction.verifying_key)?;

		ValidateCommon::validate_tx(&transaction.clone(), &sender, &db_pool_conn)
			.await
			.and_then(|_| Ok(()))?;

		// Check if the sender has exceeded the rate limit within the time frame
		let mut current_time = current_timestamp_in_millis()?;
		let time_frame_milliseconds = seconds_to_milliseconds(self.time_frame_seconds);
		// Retrieve the transactions for the sender from the mempool within the time frame
		let entries: HashMap<TimeStamp, Vec<Transaction>> = match self.transactions.get(&sender) {
			Some(sender_transactions) => sender_transactions
				.clone()
				.into_iter()
				.filter(|(timestamp, _)| *timestamp >= (current_time - time_frame_milliseconds))
				.collect(),
			None => HashMap::new(), // No transactions for the sender, so an empty HashMap
		};

		// Calculate the sender's transaction count
		let sender_transaction_count = entries.values().flatten().count();

		if sender_transaction_count >= self.rate_limit {
			return Err(anyhow!("Rate limit exceeded for the sender: {}, tx hash: {}", hex::encode(&sender), transaction_hash_string));
		}

		let conflicts: Vec<(Transaction, TimeStamp)> = self.get_conflicts(&transaction).await;
		if !conflicts.is_empty() {
			// Remove conflicting transactions from mempool
			for (conflict, conflict_timestamp) in &conflicts {
				let conflict_sender = Account::address(&conflict.verifying_key)?;
				let sender_transactions = self.transactions.get_mut(&conflict_sender);
				if let Some(sender_transactions) = sender_transactions {
					if let Some(transactions_by_timestamp) =
						sender_transactions.get_mut(conflict_timestamp)
					{
						transactions_by_timestamp.retain(|tx| tx.nonce != conflict.nonce);
					}
				}
			}
			// Handle conflicts based on transaction fee
			(transaction, current_time) =
				self.handle_conflicts(conflicts, (transaction, current_time));
		}

		let entry = self
			.transactions
			.entry(sender.clone())
			.or_insert_with(HashMap::new)
			.entry(current_time)
			.or_insert_with(Vec::new);
		entry.push(transaction.clone());

		// Determine the insertion position based on gas (higher gas first)
		let insertion_pos = self
			.transactions_priority
			.iter()
			.position(|(tx, _)| tx.fee_limit < transaction.fee_limit)
			.unwrap_or_else(|| self.transactions.len() - 1);

		// Insert the transaction at the determined position
		self.transactions_priority
			.insert(insertion_pos, (transaction.clone(), current_time));
		let size = self.transactions_priority.len();

		info!("MEMPOOL: Transaction has been added. mempool size: {}, tx hash: {}, sender: {}, nonce: {}",
			size, transaction_hash_string, hex::encode(&sender), transaction.nonce);

		if self.multinode_mode {
			if let Err(e) = self
				.network_client_tx
				.send(BroadcastNetwork::BroadcastTransaction(transaction))
				.await
			{
				warn!("Unable to write transaction ({transaction_hash_string}) to network_client_tx channel: {:?}", e)
			}
		}

		self.validate_queues("add_transaction");

		Ok(())
	}

	fn validate_queues(&self, msg: &str) -> bool {
		let transactions: HashSet<String> = HashSet::from_iter(
			self.transactions
				.values()
				.map(|map| {
					map.values()
						.map(|tx| {
							tx.iter().map(|tx| hex::encode(&tx.signature)).collect::<Vec<String>>()
						})
						.flatten()
						.collect::<Vec<String>>()
				})
				.flatten()
				.collect::<Vec<String>>(),
		);

		let trasactions_priority = HashSet::from_iter(
			self.transactions_priority
				.iter()
				.map(|v| hex::encode(&v.0.signature))
				.collect::<Vec<String>>(),
		);

		if transactions != trasactions_priority {
			warn!("validate_queues not equal, {}", msg);
			warn!("transactions, len={}", transactions.len());
			for tx in &transactions {
				warn!("{}", tx)
			}
			warn!("trasactions_priority, len={}", trasactions_priority.len());
			for tx in &trasactions_priority {
				warn!("{}", tx)
			}

			false
		} else {
			true
		}
	}

	async fn get_conflicts(&mut self, transaction: &Transaction) -> Vec<(Transaction, TimeStamp)> {
		let mut conflicts: Vec<(Transaction, TimeStamp)> = Vec::new();
		let mut transactions_priority_copy = self.transactions_priority.clone();

		// Check for conflicts with existing transactions
		transactions_priority_copy.retain(|(existing_tx, existing_tx_timestamp)| {
			if self.has_conflict(existing_tx, &transaction) {
				conflicts.push((existing_tx.clone(), *existing_tx_timestamp));
				false // Remove the conflicting transaction from the mempool_priority
			} else {
				true // Keep the non-conflicting transaction in the mempool_priority
			}
		});
		self.transactions_priority = transactions_priority_copy;
		conflicts
	}

	fn has_conflict(&self, existing_tx: &Transaction, new_tx: &Transaction) -> bool {
		let existing_tx_sender = match Account::address(&existing_tx.verifying_key) {
			Ok(address) => address,
			Err(_) => return false,
		};
		let new_tx_sender = match Account::address(&new_tx.verifying_key) {
			Ok(address) => address,
			Err(_) => return false,
		};

		existing_tx_sender == new_tx_sender && existing_tx.nonce == new_tx.nonce
	}

	fn handle_conflicts(
		&self,
		conflicts: Vec<(Transaction, TimeStamp)>,
		new_tx: (Transaction, TimeStamp),
	) -> (Transaction, TimeStamp) {
		let (new_tx_transaction, _) = new_tx.clone();
		let (mut transaction, mut timestamp) = new_tx.clone();
		for (conflict, conflict_timestamp) in conflicts {
			if new_tx_transaction.fee_limit < conflict.fee_limit {
				transaction = conflict.clone();
				timestamp = conflict_timestamp;
			}
		}
		(transaction, timestamp)
	}

	pub async fn remove_transaction_by_sender(&mut self, sender: &Address) {
		self.transactions.remove(sender);
	}

	pub async fn remove_transaction(&mut self, transaction: &Transaction) -> Result<(), Error> {
		// The transaction is stored in two places: `self.transactions` and
		// `self.transactions_priority`. Need to remove it from both
		let sender = Account::address(&transaction.verifying_key)?;

		// 1. Remove from `self.transactions`
		let mut need_to_cleenup = false;
		if let Some(transactions) = self.transactions.get_mut(&sender) {
			let mut empty_need_to_cleenup = Vec::new();
			transactions.iter_mut().for_each(|(timestamp, tx)| {
				tx.retain(|t| t.signature != transaction.signature);
				if tx.is_empty() {
					empty_need_to_cleenup.push(*timestamp)
				}
			});

			// Remove an empty record. Length of `transactions` is used in `add_transaction`
			empty_need_to_cleenup.iter().for_each(|timestamp| {
				transactions.remove(timestamp);
			});
			if transactions.is_empty() {
				need_to_cleenup = true;
			}
		}

		// Length of `transactions` is used in `add_transaction`. So need to remove empty records
		if need_to_cleenup {
			self.transactions.remove(&sender);
		}

		// Remove the transaction
		self.transactions_priority
			.retain(|(tx, _timestamp)| tx.signature != transaction.signature);

		self.validate_queues("remove_transaction");

		Ok(())
	}

	pub async fn get_transactions(&mut self) -> Vec<Transaction> {
		self.transactions
			.values()
			.flat_map(|transactions_by_timestamp| {
				transactions_by_timestamp.values().flatten().cloned()
			})
			.collect()
	}

	pub async fn get_transactions_priority(&mut self) -> Vec<Transaction> {
		self.convert_to_transactions(self.transactions_priority.clone())
	}

	fn convert_to_transactions(&self, vec: Vec<(Transaction, TimeStamp)>) -> Vec<Transaction> {
		vec.into_iter().map(|(transaction, _)| transaction).collect()
	}

	pub async fn get_transactions_by_address(&mut self, address: Address) -> Vec<Transaction> {
		if let Some(transactions_by_timestamp) = self.transactions.get(&address) {
			transactions_by_timestamp.values().flatten().cloned().collect()
		} else {
			Vec::new()
		}
	}

	pub async fn remove_expired_transactions(&mut self) -> Result<HashSet<Transaction>, Error> {
		let current_time = current_timestamp_in_millis()?;
		let mut expired_transactions: HashSet<Transaction> = HashSet::new();
		let time_expiration_milliseconds = seconds_to_milliseconds(self.expiration_seconds);
		// remove transactions from transaction list
		self.transactions.retain(|_sender, transactions_by_timestamp| {
			transactions_by_timestamp.retain(|timestamp, transactions| {
				if *timestamp >= current_time - time_expiration_milliseconds {
					true
				} else {
					// Insert each element from the vector into the hashset
					expired_transactions.extend(transactions.clone());
					false
				}
			});
			!transactions_by_timestamp.is_empty()
		});

		// remove transaction from transaction priority list
		self.transactions_priority.retain(|(tx, _timestamp)| {
			!expired_transactions.contains(&tx)
		});

		Ok(expired_transactions)
	}

	pub async fn clear_transactions(&mut self) {
		//let left_transactions = self.get_transactions().await;
		self.transactions_priority = vec![];
		self.transactions = HashMap::new();
		/*for tx in left_transactions {
			self.add_transaction(tx);
		}*/
	}

	pub async fn build_new_block_transactions_list(&self, account_state: &AccountState<'a>) -> Result<Vec<Transaction>, Error> {
		let mut transactions_to_be_included = Vec::new();
		let mut accounts = HashMap::new();
		let mut sorted_by_nonce = self.transactions_priority.iter().map(|(tx, _)| tx).collect::<Vec<_>>();
		// Sort transactions to have correct order. The priority is ignored.
		// TOOD: take the priority into account
		sorted_by_nonce.sort_by_key(|tx| tx.nonce);
		for tx in sorted_by_nonce {
			let sender = Account::address(&tx.verifying_key)?;

			let account = if let Some(account) = accounts.get_mut(&sender) {
				account
			} else {
				let account = account_state.get_account(&sender).await?;
				accounts.insert(sender.clone(), account);
				accounts.get_mut(&sender).unwrap()
			};

			if let Some(expected_nonce) = account.nonce.checked_add(1) {
				if expected_nonce == tx.nonce {
					let required_deposit = match tx.transaction_type {
						TransactionType::NativeTokenTransfer(_, amount) => amount,
						_ => 0,
					};

					if let Some(required_amount) = required_deposit.checked_add(tx.fee_limit) {
						if let Some(new_balance) = account.balance.checked_sub(required_amount) {
							account.balance = new_balance;
							account.nonce = expected_nonce;
							transactions_to_be_included.push(tx.clone())
						} else {
							warn!("Transaction is dropped: hash: {}, sender: {}, not enough balance", 
								hex::encode(&tx.transaction_hash().unwrap_or_default()), hex::encode(&sender))
						}
					} else {
						warn!("Transaction is dropped: hash: {}, sender: {}, required amount overflow", 
							hex::encode(&tx.transaction_hash().unwrap_or_default()), hex::encode(&sender))
					}
				} else {
					warn!("Transaction is dropped: hash: {}, sender: {}, invalid nonce, expected nonce: {}, tx nonce: {}", 
					hex::encode(&tx.transaction_hash().unwrap_or_default()), hex::encode(&sender), expected_nonce, tx.nonce)
				}
			} else {
				warn!("Transaction is dropped: hash: {}, sender: {}, nonce overflow", 
					hex::encode(&tx.transaction_hash().unwrap_or_default()), hex::encode(&sender))
			}
		}

		Ok(transactions_to_be_included)
	}

	pub async fn propose_block(
		&mut self,
		db_pool_conn: &'a DbTxConn<'a>,
		secret_key: SecretKey,
		verifying_key: PublicKey,
		network_receive_tx: mpsc::Sender<NetworkMessage>,
	) -> Result<ProposeBlockResult, Error> {
		let mut block_proposer_manager = BlockProposerManager {};
		let block_manager = BlockManager{};
		let block_state = BlockState::new(db_pool_conn).await?;
		let block_header = block_state.block_head_header(self.cluster_address.clone()).await?;
		let block_number = block_header.block_number + 1;

		// If the previous block is not executed then the transactions in mempool can't be validated so a new block can't be proposed.
		match block_state.is_block_executed(block_header.block_number, &self.cluster_address).await {
			Ok(is_exec) => {
				if !is_exec {
					debug!("The previous block is not executed: {}", block_header.block_number);
					return Ok(ProposeBlockResult::NotProposed)
				}
			},
			Err(e) => {
				warn!("Propose Block: is_block_executed failed, block #{}, error: {}", block_header.block_number, e);
				return Ok(ProposeBlockResult::NotProposed)
			}
		};
		
		let current_epoch = block_manager.calculate_current_epoch(block_number)?;
		let block_proposer = block_proposer_manager.get_block_proposer_for_epoch(block_number, self.cluster_address, current_epoch, db_pool_conn).await?;

        if block_proposer.address != self.node_address {
            // This node is not the proposer for this epoch
			log::debug!("Node {} is not block proposer for this epoch {}", hex::encode(self.node_address), current_epoch);
            return Ok(ProposeBlockResult::NotProposed);
        }
		info!(
			"I am proposing a block. Block Proposer: {}",
			hex::encode(block_proposer.address)
		);

		let reward_txs = self.build_reward_transactions(&block_header.block_hash, block_number -1, db_pool_conn).await?;

		// Validate transactions to be sure all of them are valid at this moment
		let account_state = AccountState::new(&db_pool_conn).await?;
		let mut transactions_to_be_included = self.build_new_block_transactions_list(&account_state).await?;
		transactions_to_be_included.extend(reward_txs);
		let block_state = BlockState::new(&db_pool_conn).await?;
		let block_manager = BlockManager::new();
		let block = block_manager
			.create_regular_block(
				transactions_to_be_included,
				self.cluster_address,
				&block_state,
				&account_state,
			)
			.await?;

		info!(
			"Block Produced, Block Hash - {:?}",
			hex::encode(block.block_header.block_hash.clone())
		);

		let mut events = Vec::new();

		// If not in multinode mode, just execute the block without broadcasting
		if !self.multinode_mode {
			self.clear_transactions().await;
			block_state.store_block(block.clone()).await?;
			events =
				ExecuteBlock::execute_block(&block, self.event_tx.clone(), db_pool_conn).await?;
			info!(
				"Block Executed, Block Hash - {:?}",
				hex::encode(block.block_header.block_hash.clone())
			);
		} 

		// If in multinode mode, broadcast the block, validators and block proposers and vote
		if self.multinode_mode {
			let json_str =
				serde_json::to_string(&block).expect("Unable to parse to json string");
			let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());
			let sig = secret_key.sign_ecdsa(message);
			let block_payload = BlockPayload {
				block: block.clone(),
				signature: sig.serialize_compact().to_vec(),
				verifying_key: verifying_key.serialize().to_vec(),
				sender: self.node_address,
			};

			let (sender, receiver) = oneshot::channel();
			if let Err(e) = network_receive_tx.send(NetworkMessage::BlockProposerEvent(BlockProposerEventType::AddBlock(block_payload, sender)))
				.await
			{
				warn!("Unable to write block_payload to network_receive_tx channel: {:?}", e)
			} else {
				match receiver.await {
					Ok(res) => match res {
						Ok(network_ack) => {
							match network_ack {
								NetworkAcknowledgement::Success => {
									self.clear_transactions().await;
									info!("Block #{} has been proposed", block.block_header.block_number);
								},
								NetworkAcknowledgement::Failure => {
									warn!("Failed to propose Block #{}", block.block_header.block_number);
									return Ok(ProposeBlockResult::NotProposed);
								},
							}
						},
						Err(e) => warn!("Received error: {:?}", e),
					},
					Err(e) => warn!("Failed to receive message: {:?}", e),
				}
			}
		}
		Ok(ProposeBlockResult::Proposed(events))
	}

	async fn build_reward_transactions(
		&self,
		block_hash: &BlockHash,
		block_number: BlockNumber,
		db_pool_conn: &'a DbTxConn<'a>
	) -> Result<Vec<Transaction>, Error> {
		let rt_config = runtime_config::RuntimeConfigCache::get().await?;
		let reward_amount = rt_config.rewards.validated_block_reward;
		if reward_amount.0 == 0 {
			return Ok(vec![]);
		}
		let rt_stake_info = runtime_config::RuntimeStakingInfoCache::get().await?;
		let vote_result_state = vote_result::vote_result_state::VoteResultState::new(&db_pool_conn).await?;
		let mut reward_txs= Vec::new();
		if let Ok(vote_result) = vote_result_state.load_vote_result(block_hash).await {
			let account_state = AccountState::new(&db_pool_conn).await?;
			let mut nonce = account_state.get_nonce(&SYSTEM_REWARDS_DISTRIBUTOR).await?;

			// generate dummy native_token_transfer transaction for fetching fee_limit
			let dummy_tx = Transaction::new_system(
				&Default::default(),
				Default::default(),
				TransactionType::NativeTokenTransfer(Address::default(), Balance::default()),
				Default::default(),
			);
			let mut execution_fee = execute_fee::ExecuteFee::new(&dummy_tx, execute_fee::SystemFeeConfig::new());

			let fee_limit = execution_fee.get_token_transfer_tx_total_fee()?;

			// Create transactions
			for vote in &vote_result.data.votes {
				let validator_address = Account::address(&vote.verifying_key)?;
				// skip org_nodes from rewards distribution
				if !rt_config.org_nodes.contains(&validator_address) {
					if let Some(info) = rt_stake_info.nodes.get(&validator_address) {
						nonce += 1;
						let transaction_type = TransactionType::NativeTokenTransfer(
							info.reward_wallet_address,
							reward_amount.into(),
						);

						let tx = Transaction::new_system(&SYSTEM_REWARDS_DISTRIBUTOR, nonce, transaction_type, fee_limit);
						reward_txs.push(tx);
					} else {
						warn!("No stake info found for vote: {:?}", vote);
					}
				}
			}

			if !reward_txs.is_empty() {
				// Calculate total rewards and fees
				let total_votes = reward_txs.len() as u128;
				let total_rewards = reward_amount.0.checked_mul(total_votes)
					.ok_or_else(|| anyhow::anyhow!("Overflow while calculating total rewards"))?;

				let total_fees = fee_limit.checked_mul(total_votes)
					.ok_or_else(|| anyhow::anyhow!("Overflow while calculating total fees"))?;
				let total_cost = total_rewards.checked_add(total_fees)
					.ok_or_else(|| anyhow::anyhow!("Overflow while calculating total cost"))?;

				// System account's balance
				let balance = account_state.get_balance(&SYSTEM_REWARDS_DISTRIBUTOR).await.unwrap_or(0);

				// Check if the balance is sufficient
				if balance < total_cost {
					warn!(
						"Not enough balance to distribute rewards for block #{}. Required: {}, Available: {}",
						block_number, total_cost, balance
					);
					return Ok(vec![]);
				}
			}
		} else {
			warn!("Vote result not found found for block: {}", block_number);
		}
		Ok(reward_txs)
	}
}
