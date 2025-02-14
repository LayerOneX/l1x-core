use std::str::FromStr;
use ::account::account_state::AccountState;
use anyhow::{anyhow, Error};
use block::block_state::BlockState;
use compile_time_config::SYSTEM_CONTRACTS_OWNER;
use db::db::{Database, DbTxConn};
use execute::execute_block::ExecuteBlock;
use l1x_rpc::rpc_model::{node_client::NodeClient, GetLatestBlocksRequest,
                         GetProtocolVersionRequest, GetBlockWithDetailsByNumberRequest,
};
use log::{error, info, warn};
use primitives::{Address, BlockNumber};
use regex::Regex;
use system::account::Account;
use system::{block::Block, network::EventBroadcast};
use tokio::sync::{broadcast, mpsc};
use tonic::transport::Channel;
use util::convert;
use db::postgres::pg_models::*;
use serde::{Serialize, Deserialize};
use db::postgres::postgres::PgConnectionType;
use diesel::{self, prelude::*};
use db::postgres::schema::{account, block_head, block_header, block_meta_info, block_proposer, block_transaction,
                           cluster, contract, contract_instance, contract_instance_contract_code_map, event, staking_account, staking_pool, sub_cluster, validator as validator_schema, vote, vote_result as vote_result_schema, genesis_epoch_block};
use diesel::r2d2::ConnectionManager;
use diesel::r2d2::PooledConnection;
use secp256k1::{Secp256k1, PublicKey};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use sha2::{Sha256, Digest};
use secp256k1::ecdsa::Signature;
use std::process::Command;
use std::time::Instant;
use reqwest::Client;
use system::config::{Config, Db};
use compile_time_config::PROTOCOL_VERSION;
use runtime_config::RuntimeConfigCache;
use system::validator::Validator;
use system::vote_result::VoteResult;
use util::convert::from_proto_vote_result;
use block::block_manager::BlockManager;
use validator::validator_state::ValidatorState;
use vote_result::vote_result_state::VoteResultState;

pub const DB_DUMP: &str = "db_dump.dump";
pub const DB_DUMP_SIGNATURE: &str = "signature.bin";
pub const CHAIN_STATE: &str = "chain_state.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TableName {
    Account,
    BlockHead,
    BlockHeader,
    BlockMetaInfo,
    BlockProposer,
    BlockTransaction,
    Cluster,
    Contract,
    ContractInstance,
    ContractInstanceContractCodeMap,
    Event,
    StakingAccount,
    StakingPool,
    SubCluster,
    Validator,
    Vote,
    VoteResult,
    GenesisEpochBlock,
}

impl TableName {
    pub fn get_all_table_names() -> Vec<TableName> {
        vec![
            TableName::Account,
            TableName::BlockHead,
            TableName::BlockHeader,
            TableName::BlockMetaInfo,
            TableName::BlockProposer,
            TableName::BlockTransaction,
            TableName::Cluster,
            TableName::Contract,
            TableName::ContractInstance,
            TableName::ContractInstanceContractCodeMap,
            TableName::Event,
            TableName::StakingAccount,
            TableName::StakingPool,
            TableName::SubCluster,
            TableName::Validator,
            TableName::Vote,
            TableName::VoteResult,
            TableName::GenesisEpochBlock,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct RemoteChainState {
    pub chain_height: BlockNumber,
    pub last_executed_block: BlockNumber,
}

async fn verify_endpoints_protocol_version(grpc_clients: &mut Vec<(NodeClient<Channel>, String)>) -> Result<(), Error> {
    for grpc_client in grpc_clients {
        let response = grpc_client.0.get_protocol_version(GetProtocolVersionRequest {}).await?;
        let protocol_version_response = response.into_inner();
        let boot_version = protocol_version_response.protocol_version;
        info!("Boot node's protocol version: {}", boot_version);
    
        if boot_version != PROTOCOL_VERSION {
           return if boot_version > PROTOCOL_VERSION {
               Err(anyhow!(
                    "Node is outdated, please upgrade. Current version: {}, Required version: {}, boot node: {}",
                    PROTOCOL_VERSION,
                    boot_version,
                    grpc_client.1,
                ))
           } else {
               Err(anyhow!(
                    "Boot node is outdated, please wait for an upgrade. Current version: {}, Boot node's version: {}, boot node: {}",
                    PROTOCOL_VERSION,
                    boot_version,
                    grpc_client.1,
                ))
           };
        }
    }

    Ok(())
}

async fn find_highest_local_executed_block(from: BlockNumber, cluster_address: &Address, block_state: &BlockState<'_>) -> Result<BlockNumber, Error> {
    if from == 0 {
        return Ok(0);
    }
    
    // Find the highest executed block in storage
    let mut start_execute_block_number = from;
    loop {
        match block_state.is_block_executed(start_execute_block_number, cluster_address).await {
            Ok(true) => {
                info!("Highest executed block is #{}", start_execute_block_number);
                break;
            }
            Ok(false) => {
                start_execute_block_number = start_execute_block_number - 1;

                if start_execute_block_number == 1 {
                    break;
                }
            }
            Err(e) => {
                error!("Can't fetch the local block #{}, error: {e}", start_execute_block_number);

                start_execute_block_number = start_execute_block_number - 1;

                if start_execute_block_number == 1 {
                    break;
                }
            }
        }
    }

    Ok(start_execute_block_number)
}

async fn get_hishest_executed_chain_state(grpc_clients: &mut Vec<(NodeClient<Channel>, String)>) -> Result<RemoteChainState, Error> {
    let mut states = Vec::new();
    for grpc_client in grpc_clients {
        let response = grpc_client.0.get_latest_blocks(GetLatestBlocksRequest {}).await?;
    
        let result = response.into_inner();
        info!("Chain state {} => {:?}", grpc_client.1, result);
        
        let chain_height: BlockNumber = result.head_block_number.parse()?;
        let chain_last_executed_block: BlockNumber = result.last_executed_block.parse()?;

        states.push(RemoteChainState{chain_height, last_executed_block: chain_last_executed_block});
    }

    let max_executed_state = states.iter().max_by(|a, b| a.last_executed_block.cmp(&b.last_executed_block))
        .expect("Can't find max executed chain state");

    Ok(max_executed_state.clone())
}

/// Syncs the node with the chain using the provided full/archive node
pub async fn sync_node(
    cluster_address: Address,
    endpoints: &[&str],
    event_tx: broadcast::Sender<EventBroadcast>,
    batch_size: u16,
) -> Result<(), Error> {
    let sync_endpoints = multiaddrs_to_http_urls(endpoints);

    // Connect to the nodes
    let mut grpc_clients = Vec::new();
    for endpoint in sync_endpoints {
        match NodeClient::connect(endpoint.clone()).await {
            Ok(grpc_client) => {
                info!("Connection successful, syncing blocks from endpoint: {}", endpoint);
                let grpc_client = grpc_client.max_decoding_message_size(usize::MAX);
                grpc_clients.push((grpc_client, endpoint));
            },
            Err(err) => {
                warn!("Failed to connect to endpoint {}: {:?}", endpoint, err);
            }
        }
    }
    if grpc_clients.is_empty() {
        return Err(anyhow!("Unable to sync. Boot nodes are not available"));
    }

    // Verify protocol version
    verify_endpoints_protocol_version(&mut grpc_clients).await?;

    // Get the latest block number from the archive node
    let (chain_height, chain_last_executed_block) = {
        let state = get_hishest_executed_chain_state(&mut grpc_clients).await?;
        info!("Highest executed chain state: {:?}", state);
        (state.chain_height, state.last_executed_block)
    };

    let db_pool_conn = Database::get_pool_connection().await?;
    let block_state = BlockState::new(&db_pool_conn).await?;

    // Highest block number in this nodes storage
    let highest_local_block = match block_state.block_head(cluster_address.clone()).await {
        Ok(block) => {
            info!("Highest locally stored block is #{}", block.block_header.block_number);
            Some(block.block_header.block_number)
        }
        Err(e) => {
            // If the block head is not found, start syncing from block 1
            if e.to_string().contains("No row for block header") || e.to_string().contains("No matching records found for") {
                if chain_height == 1 {
                    return Ok(());
                }
                None
            } else {
                return Err(anyhow!("Error getting block head when starting node sync: {:?}", e));
            }
        }
    };

    let (start_download_block_number, highest_local_executed_block) = match highest_local_block {
        Some(block_number) => {
            // Find the highest EXECUTED block number from this nodes storage
            let highest_local_executed_block = find_highest_local_executed_block(block_number, &cluster_address, &block_state).await?;
            (block_number + 1, highest_local_executed_block)
        }
        None => (1, 0) // Currently we don't have a block 0, the chain starts at 1
    };

    info!("The highest local executed block #{}", highest_local_executed_block);

    if highest_local_executed_block >= chain_last_executed_block {
        info!("🎉 The local chain is up-to-date");
        return Ok(())
    }

    let start_execute_block_number = highest_local_executed_block + 1;

    info!("Beginning execution from block #{}", start_execute_block_number);
    
    // Channel for send block batches to be verified and stored
    let (block_batch_tx, block_batch_rx) = mpsc::channel(10_000);

    // Start the block validate and store process
    let store_validate_blocks = tokio::spawn(async move { validate_and_store_blocks(chain_last_executed_block, block_batch_rx).await });

    // Start the block execution process
    let execute_blocks = tokio::spawn(async move {
        execute_blocks(
            start_execute_block_number,
            chain_last_executed_block,
            batch_size,
            cluster_address,
            event_tx,
        ).await
    });

    // Cycle iterator of all connections. This will help us to distribute the load of block
    // downloads
    let mut grpc_clients_iter = grpc_clients.iter().cycle();
    let mut block_number = start_download_block_number;

    // Download blocks in batches until we reach the initialy found chain height. During the sync,
    // blocks will be produced with a higher block number than the initial chain height. These will
    // be picked up by the normal p2p network and will be buffered. When the sync is done, the
    // buffered blocks will be processed.
    while block_number <= chain_last_executed_block {
        // Download and store block_batch_size blocks in parallel
        let mut tasks = Vec::new();

        // Store the missing blocks from a batch so we can try to download them again
        let mut missing_blocks: Vec<BlockNumber> = Vec::new();

        // When remaining blocks are less than batch_size
        let mut batch_upper_limit = block_number + batch_size as u128;
        if batch_upper_limit > chain_last_executed_block {
            batch_upper_limit = chain_last_executed_block + 1;
            println!(
                "batch_upper_limit > last_executed_block\n batch_upper_limit = {}",
                batch_upper_limit
            );
        }

        info!("📥 Syncing: Downloading blocks #{} - #{}, batch size: {}", block_number, batch_upper_limit - 1, batch_size);

        // Runtime can be not intialized yet if the system config contract is not deployed yet
        let max_block_height = RuntimeConfigCache::get().await
            .and_then(|c| {
            Ok(c.max_block_height)
            })
            .ok()
            .unwrap_or_default();

        let block_manager = BlockManager{};
        let mut fetch_validator = true;
        // Download a batch of blocks
        for _ in block_number..batch_upper_limit {
            let (grpc_client, endpoint) = grpc_clients_iter.next().unwrap().clone();
            if let Some(max_block_height) = max_block_height {
                if block_number >= max_block_height as BlockNumber {
                    error!("Node has reached the maximum configured block height: {}. Please upgrade or modify the configuration to continue.", max_block_height);
                    return Err(anyhow!("Blockchain has reached the configured stopping block number: {}", max_block_height));
                }
            }

            let include_validators = fetch_validator || block_manager.has_epoch_ended(block_number)?;
            fetch_validator = false;

            let task = tokio::spawn(async move { fetch_block(block_number, true, include_validators, endpoint, grpc_client).await });
            tasks.push(task);
            block_number = block_number + 1;
        }

        // Now tasks.len() == batch_size
        // Wait for all tasks to complete
        let results = futures::future::join_all(tasks).await;

        // Store the received blocks
        let mut block_batch: Vec<(Block, Option<VoteResult>, Vec<Validator>)> = Vec::new();

        // Unwrap results and push blocks into vec
        for result in results {
            match result {
                Ok(block) => {
                    match block {
                        Ok((block_number, block_option, vote_result, validators)) => {
                            match block_option {
                                Some(block) => {
                                    block_batch.push((block, vote_result, validators));
                                }
                                None => {
                                    missing_blocks.push(block_number);
                                    error!("Block {} is missing during sync", block_number);
                                }
                            }
                        },
                        Err((block_number, e)) => {
                            missing_blocks.push(block_number);
                            error!("Block {} is missing during sync", block_number);
                        }
                    }
                }
                Err(e) => return Err(anyhow!("Block sync results error: {:?}", e)),
            }
        }

        info!("📥 Syncing: Downloaded block batch of size {}", block_batch.len());

        // If there are missing blocks, retry downloading them
        if !missing_blocks.is_empty() {
            warn!("🚨🚨🚨 Retrying download of missing blocks: {:?}", missing_blocks);
            let found_blocks = get_missing_blocks(missing_blocks, true, true, grpc_clients.clone()).await;
            block_batch.extend(found_blocks);
        }

        block_batch_tx.send(block_batch).await.map_err(|e| {
            anyhow!("Error sending block batch to validate and store process: {}", e)
        })?;
    }

    info!("📥 Syncing: Block download finished");

    if let Err(e) = store_validate_blocks.await? {
        return Err(anyhow!("Error validating and storing sync blocks: {:?}", e));
    }
    info!("💾 Finished storing blocks");

    if let Err(e) = execute_blocks.await? {
        return Err(anyhow!("Error executing sync blocks: {:?}", e));
    }

    info!("🎉 Finished executing sync blocks");

    Ok(())
}

async fn execute_blocks(
    start_execute_block_number: BlockNumber,
    chain_height: BlockNumber,
    // batch size will be used in the future
    _batch_size: u16,
    cluster_address: Address,
    event_tx: broadcast::Sender<EventBroadcast>,
) -> Result<(), Error> {
    let db_pool_conn = Database::get_pool_connection().await?;
    let block_state = BlockState::new(&db_pool_conn).await?;

    for block_number in start_execute_block_number..=chain_height {
        let block: Block;
        loop {
            match block_state.load_block(block_number, &cluster_address).await {
                Ok(stored_block) => {
                    block = stored_block;
                    break;
                }
                Err(_) => {
                    warn!(
                        "⏱️ Block #{} not found in storage. Waiting for block to be stored...",
                        block_number
                    );

                    // Wait some to give the blocks time for download, verification, and storage
                    tokio::time::sleep(tokio::time::Duration::from_millis(1_000)).await;
                    continue;
                }
            };
        }

        // Validate transactions in the block
        for tx in &block.transactions {
            let sender = system::account::Account::address(&tx.verifying_key)?;
            if (block.is_system_block() && sender == compile_time_config::SYSTEM_CONTRACTS_OWNER) || sender == compile_time_config::SYSTEM_REWARDS_DISTRIBUTOR {
                continue;
            }
            if let Err(e) = validate::validate_common::ValidateCommon::validate_tx(&tx, &sender, &db_pool_conn).await {
                error!("Transaction validation failed, block #{}: {}", block.block_header.block_number, e);
                Err(e)?
            }
        }

        // Check whether the parent block is executed for safety
        if block_number > 1 {
            let parent_block = block_number - 1;
            let is_executed = block_state.is_block_executed(parent_block, &cluster_address).await.unwrap_or(false);
            if !is_executed {
                let msg = format!("The parent block #{} has not been executed", parent_block);
                error!("{msg}");
                Err(anyhow!("{msg}"))?;
            }
        }

        if PROTOCOL_VERSION == 3 && block.is_system_block() {
            let account_state = AccountState::new(&db_pool_conn).await?;
            match account_state.is_valid_account(&SYSTEM_CONTRACTS_OWNER).await {
                Ok(false)
                | Err(_) => {
                    // Create the system account if it doesn't exist
                    let system_account = Account::new_system(SYSTEM_CONTRACTS_OWNER);
                    account_state.create_account(&system_account).await?;
                }
                Ok(true) => ()
            }
        }

        // Don't worry about returned events during syncing
        let ret = ExecuteBlock::execute_block(&block, event_tx.clone(), &db_pool_conn).await;
        match ret {
            Err(e) => {
                error!("Failed to execute block #{}: {}\n{:?}", block.block_header.block_number, e, block);
                Err(e)?
            },
            Ok(_) => ()
        }

        info!(
			"✅ Block #{} executed, out of sync start chain height of #{}",
			block_number, chain_height
		);
    }

    Ok(())
}

async fn validate_and_store_blocks(
    chain_height: BlockNumber,
    mut block_batch_rx: mpsc::Receiver<Vec<(Block, Option<VoteResult>, Vec<Validator>)>>,
) -> Result<(), Error> {
    let db_pool_conn = Database::get_pool_connection().await?;
    let block_state = BlockState::new(&db_pool_conn).await?;
    let vote_result_state = VoteResultState::new(&db_pool_conn).await?;
    let validator_state = ValidatorState::new(&db_pool_conn).await?;

    // Await new batches of blocks
    while let Some(mut block_batch) = block_batch_rx.recv().await {
        info!("🔎🔎🔎 Validating and storing block batch of size {}", block_batch.len());

        // Validate the blocks
        // todo!("Validate the blocks");

        // Sort the blocks by block number
        block_batch.sort_by_key(|block| block.0.block_header.block_number);
        let last_block_number = match block_batch.last() {
            Some(block) => block.0.block_header.block_number,
            None => return Err(anyhow!("No blocks in batch, shouldn't happen")),
        };

        let blocks: Vec<Block> = block_batch.clone().into_iter().map(|(block, _, _)| block).collect();

        let vote_results: Vec<VoteResult> = block_batch
            .iter()
            .filter_map(|(_, vote_result, _)| vote_result.clone())
            .collect();

        let validators: Vec<Validator> = block_batch
            .iter()
            .flat_map(|(_, _, validator_list)| validator_list.clone())
            .collect();

        // Store a batch of blocks in the database
        block_state.batch_store_blocks(blocks).await.map_err(|e| anyhow!("Error batch storing blocks: {:?}", e))?;
        vote_result_state.batch_store_vote_results(&vote_results).await.map_err(|e| anyhow!("Error batch storing vote results: {:?}", e))?;
        validator_state.batch_store_validators(&validators).await.map_err(|e| anyhow!("Error batch storing validators: {:?}", e))?;

        if last_block_number == chain_height {
            warn!(
				"🚨🚨🚨🚨🚨🚨🚨🚨🚨🚨🚨🚨Exiting validate and sync loop. Channel will be closed."
			);
            break;
        }
    }
    info!("Block batch channel closed");

    Ok(())
}

async fn get_missing_blocks(
    block_numbers: Vec<BlockNumber>,
    include_vote_result: bool,
    include_validators: bool,
    grpc_clients: Vec<(NodeClient<Channel>, String)>,
) -> Vec<(Block, Option<VoteResult>, Vec<Validator>)> {
    let mut grpc_clients_iter = grpc_clients.iter().cycle();

    let mut tasks = Vec::new();

    for block_number in block_numbers {
        let (grpc_client, endpoint) = grpc_clients_iter.next().unwrap().clone();
        let task = tokio::spawn(async move { fetch_block(block_number, include_vote_result, include_validators, endpoint, grpc_client).await });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    let results = futures::future::join_all(tasks).await;
    let mut found_blocks: Vec<(Block, Option<VoteResult>, Vec<Validator>)> = Vec::new();

    for result in results {
        match result {
            Ok(block) => match block {
                Ok(block) => {
                    let (block_number, block_option, vote_result, validators) = block;
                    match block_option {
                        Some(block) => {
                            found_blocks.push((block, vote_result, validators));
                        }
                        None => {
                            panic!("Block {} is missing during sync", block_number);
                        }
                    }
                }
                Err((block_number, e)) => panic!("Error syncing missing block #{}: {}", block_number, e),
            },
            Err(e) => panic!("Error syncing missing block(s): {}", e),
        }
    }

    found_blocks
}

pub async fn fetch_block(
    block_number: BlockNumber,
    include_vote_result: bool,
    include_validators: bool,
    endpoint: String,
    grpc_client: NodeClient<Channel>,
) -> Result<(BlockNumber, Option<Block>, Option<VoteResult>, Vec<Validator>), (BlockNumber, Error)> {
    let mut grpc_client = grpc_client;
    let response;
    // We need retries because load balancer is used. 
    // Because chain height of nodes can be different, we need to wait until oudated nodes are update their states 
    let mut retry = 1;
    let max_tries = 5;
    info!("Syncing block #{} from {}", block_number, endpoint);
    loop {
        match grpc_client.get_block_with_details_by_number(GetBlockWithDetailsByNumberRequest { block_number: block_number.to_string(), include_vote_result, include_validators }).await {
            Ok(res) => {
                response = res;
                break;
            }
            Err(e) => {
                retry += 1;
                if retry > max_tries {
                    return Err((block_number, anyhow!("Failed to fetch block #{} from {}, error: {}. Reached max tries ({})", block_number, endpoint, e, max_tries)));
                }
                let sleep_time = retry * 2000;
                warn!("Failed to fetch block #{} from {}, error: {}. Try {}/{}, sleep {}ms", block_number, endpoint, e, retry, max_tries, sleep_time);
                tokio::time::sleep(tokio::time::Duration::from_millis(sleep_time)).await;
            }
        }
    }
    let response = response.into_inner();
    let block = match response.block {
        Some(proto_block) => {
            convert::from_proto_block_v3(proto_block)
                .await
                .map_err(|e| {
                    let error_message = format!("Failed to convert block #{} from proto, error: {:?}", block_number, e);
                    (block_number, anyhow!(error_message))
                })?
        }
        None => {
            info!("Block {} is missing during sync", block_number);
            return Ok((block_number, None, None, vec![]));
        }
    };

    let vote_result = match response.vote_result {
        Some(proto_vote_result) => Some(
            from_proto_vote_result(block.clone(), proto_vote_result)
                .await
                .map_err(|e| {
                let error_message = format!("Failed to convert block #{} from proto, error: {:?}", block_number, e);
                (block_number, anyhow!(error_message))
            })?),
        None => {
            info!("Vote result for block {} is missing during sync", block_number);
            None
        }
    };

    let validators: Result<Vec<Validator>, (BlockNumber, anyhow::Error)> = response
        .validators
        .into_iter()
        .map(|rpc_validator| {
            rpc_validator
                .address
                .try_into()
                .map(|address| Validator {
                    address,
                    cluster_address: block.block_header.cluster_address.clone(),
                    epoch: rpc_validator.epoch,
                    stake: Default::default(),
                    xscore: rpc_validator.xscore,
                })
                .map_err(|_| (block_number, anyhow!("Unable to convert validator_address from rpc_validator")))
        })
        .collect();

    let validators = validators?;

    Ok((block_number, Some(block), vote_result, validators))
}

/// Converts an array of multiaddresses to HTTP URLs.
///
/// This function takes an array of multiaddresses as input and converts each multiaddress
/// into an HTTP URL. It uses a regular expression pattern to match and extract the IP address
/// from each multiaddress. If an IP address is found, it constructs an HTTP URL using the
/// extracted IP address and adds it to a vector of HTTP URLs. If no IP address is found in
/// a multiaddress, it prints a message indicating that no IP address was found.
///
/// # Arguments
///
/// * `multiaddr` - An array of multiaddresses.
///
/// # Returns
///
/// A vector of HTTP URLs.
pub fn multiaddrs_to_http_urls(multiaddr: &[&str]) -> Vec<String> {
    // Regular expression pattern to match an IP address
    let re = Regex::new(r"/ip4/(\d+\.\d+\.\d+\.\d+)/").unwrap();

    // Extracting the IP address
    let mut http_urls: Vec<String> = Vec::new();
    for addr in multiaddr {
        if let Some(capture) = re.captures(addr) {
            let ip_address = capture.get(1).unwrap().as_str();
            let http_url = format!("http://{}:50052", ip_address);
            http_urls.push(http_url);
        } else {
            println!("No IP address found in the input string.");
        }
    }

    info!("Syncing node from endpoints: {:?}", http_urls);
    http_urls
}

async fn insert_data(conn: &mut PooledConnection<ConnectionManager<PgConnection>>, table_name: TableName, data: &[u8], counter: &mut (TableName, u64)) -> Result<(), Error> {
    match table_name {
        TableName::Account => {
            let accounts: Vec<NewAccount> = serde_json::from_slice(data)?;
            if counter.0 == TableName::Account {
                counter.1 = counter.1 + accounts.len() as u64
            } else {
                counter.0 = TableName::Account;
                counter.1 = accounts.len() as u64
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for account in accounts {
                if let Err(err) = diesel::insert_into(account::table).values(account).execute(conn) {
                    error!("Error while storing record: {:?}", err);
                }
            }
        }
        TableName::BlockHead => {
            let block_heads: Vec<QueryBlockHead> = serde_json::from_slice(data)?;
            if counter.0 == TableName::BlockHead {
                counter.1 = counter.1 + block_heads.len() as u64
            } else {
                counter.0 = TableName::BlockHead;
                counter.1 = block_heads.len() as u64
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for block_head in block_heads {
                diesel::insert_into(block_head::table).values(block_head).execute(conn);
            }
        }
        TableName::BlockHeader => {
            let block_headers: Vec<QueryBlockHeader> = serde_json::from_slice(data)?;
            if counter.0 == TableName::BlockHeader {
                counter.1 = counter.1 + block_headers.len() as u64;
            } else {
                counter.0 = TableName::BlockHeader;
                counter.1 = block_headers.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for block_header in block_headers {
                diesel::insert_into(block_header::table).values(block_header).execute(conn);
            }
        }
        TableName::BlockMetaInfo => {
            let block_meta_infos: Vec<QueryBlockMetaInfo> = serde_json::from_slice(data)?;
            if counter.0 == TableName::BlockMetaInfo {
                counter.1 = counter.1 + block_meta_infos.len() as u64;
            } else {
                counter.0 = TableName::BlockMetaInfo;
                counter.1 = block_meta_infos.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for block_meta_info in block_meta_infos {
                diesel::insert_into(block_meta_info::table).values(block_meta_info).execute(conn);
            }
        }
        TableName::BlockProposer => {
            let block_proposers: Vec<QueryBlockProposer> = serde_json::from_slice(data)?;
            if counter.0 == TableName::BlockProposer {
                counter.1 = counter.1 + block_proposers.len() as u64;
            } else {
                counter.0 = TableName::BlockProposer;
                counter.1 = block_proposers.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for block_proposer in block_proposers {
                diesel::insert_into(block_proposer::table).values(block_proposer).execute(conn);
            }
        }
        TableName::BlockTransaction => {
            let block_transactions: Vec<QueryBlockTransactions> = serde_json::from_slice(data)?;
            if counter.0 == TableName::BlockTransaction {
                counter.1 = counter.1 + block_transactions.len() as u64;
            } else {
                counter.0 = TableName::BlockTransaction;
                counter.1 = block_transactions.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for block_transaction in block_transactions {
                diesel::insert_into(block_transaction::table).values(block_transaction).execute(conn);
            }
        }
        TableName::Cluster => {
            let clusters: Vec<Cluster> = serde_json::from_slice(data)?;
            if counter.0 == TableName::Cluster {
                counter.1 = counter.1 + clusters.len() as u64;
            } else {
                counter.0 = TableName::Cluster;
                counter.1 = clusters.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for cluster in clusters {
                diesel::insert_into(cluster::table).values(cluster).execute(conn);
            }
        }
        TableName::Contract => {
            let contracts: Vec<QueryContract> = serde_json::from_slice(data)?;
            if counter.0 == TableName::Contract {
                counter.1 = counter.1 + contracts.len() as u64
            } else {
                counter.0 = TableName::Contract;
                counter.1 = contracts.len() as u64
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for contract in contracts {
                diesel::insert_into(contract::table).values(contract).execute(conn);
            }
        }
        TableName::ContractInstance => {
            let contract_instances: Vec<QueryContractInstance> = serde_json::from_slice(data)?;
            if counter.0 == TableName::ContractInstance {
                counter.1 = counter.1 + contract_instances.len() as u64;
            } else {
                counter.0 = TableName::ContractInstance;
                counter.1 = contract_instances.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for contract_instance in contract_instances {
                diesel::insert_into(contract_instance::table).values(contract_instance).execute(conn);
            }
        }
        TableName::ContractInstanceContractCodeMap => {
            let contract_instance_contract_code_maps: Vec<QueryContractInstanceContractCodeMap> = serde_json::from_slice(data)?;
            if counter.0 == TableName::ContractInstanceContractCodeMap {
                counter.1 = counter.1 + contract_instance_contract_code_maps.len() as u64;
            } else {
                counter.0 = TableName::ContractInstanceContractCodeMap;
                counter.1 = contract_instance_contract_code_maps.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for contract_instance_contract_code_map in contract_instance_contract_code_maps {
                diesel::insert_into(contract_instance_contract_code_map::table).values(contract_instance_contract_code_map).execute(conn);
            }
        }
        TableName::Event => {
            let events: Vec<QueryEvent> = serde_json::from_slice(data)?;
            if counter.0 == TableName::Event {
                counter.1 = counter.1 + events.len() as u64;
            } else {
                counter.0 = TableName::Event;
                counter.1 = events.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for event in events {
                diesel::insert_into(event::table).values(event).execute(conn);
            }
        }
        TableName::StakingAccount => {
            let staking_accounts: Vec<QueryStakingAccount> = serde_json::from_slice(data)?;
            if counter.0 == TableName::StakingAccount {
                counter.1 = counter.1 + staking_accounts.len() as u64;
            } else {
                counter.0 = TableName::StakingAccount;
                counter.1 = staking_accounts.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for staking_account in staking_accounts {
                diesel::insert_into(staking_account::table).values(staking_account).execute(conn);
            }
        }
        TableName::StakingPool => {
            let staking_pools: Vec<QueryStakingPool> = serde_json::from_slice(data)?;
            if counter.0 == TableName::StakingPool {
                counter.1 = counter.1 + staking_pools.len() as u64;
            } else {
                counter.0 = TableName::StakingPool;
                counter.1 = staking_pools.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for staking_pool in staking_pools {
                diesel::insert_into(staking_pool::table).values(staking_pool).execute(conn);
            }
        }
        TableName::SubCluster => {
            let sub_clusters: Vec<SubCluster> = serde_json::from_slice(data)?;
            if counter.0 == TableName::SubCluster {
                counter.1 = counter.1 + sub_clusters.len() as u64;
            } else {
                counter.0 = TableName::SubCluster;
                counter.1 = sub_clusters.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for sub_cluster in sub_clusters {
                diesel::insert_into(sub_cluster::table).values(sub_cluster).execute(conn);
            }
        }
        TableName::Validator => {
            let validators: Vec<QueryValidator> = serde_json::from_slice(data)?;
            if counter.0 == TableName::Validator {
                counter.1 = counter.1 + validators.len() as u64;
            } else {
                counter.0 = TableName::Validator;
                counter.1 = validators.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for validator in validators {
                diesel::insert_into(validator_schema::table).values(validator).execute(conn);
            }
        }
        TableName::Vote => {
            let votes: Vec<QueryVote> = serde_json::from_slice(data)?;
            if counter.0 == TableName::Vote {
                counter.1 = counter.1 + votes.len() as u64;
            } else {
                counter.0 = TableName::Vote;
                counter.1 = votes.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for vote in votes {
                diesel::insert_into(vote::table).values(vote).execute(conn);
            }
        }
        TableName::VoteResult => {
            let vote_results: Vec<QueryVoteResult> = serde_json::from_slice(data)?;
            if counter.0 == TableName::VoteResult {
                counter.1 = counter.1 + vote_results.len() as u64;
            } else {
                counter.0 = TableName::VoteResult;
                counter.1 = vote_results.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for vote_result in vote_results {
                diesel::insert_into(vote_result_schema::table).values(vote_result).execute(conn);
            }
        }
        TableName::GenesisEpochBlock => {
            let genesis: Vec<QueryGenesisBlock> = serde_json::from_slice(data)?;
            if counter.0 == TableName::GenesisEpochBlock {
                counter.1 = counter.1 + genesis.len() as u64;
            } else {
                counter.0 = TableName::GenesisEpochBlock;
                counter.1 = genesis.len() as u64;
            }
            info!("Storing table record: table name: {:?}, total row counter: {:?}", counter.0, counter.1);
            for gen in genesis {
                diesel::insert_into(genesis_epoch_block::table).values(gen).execute(conn);
            }
        }
    };
    Ok(())
}

async fn clear_db_records() -> Result<(), Error> {
    let connection = Database::get_pool_connection().await.map_err(|e| {
        anyhow!("Unable to get db connection: {}", e)
    })?;

    match connection {
        DbTxConn::POSTGRES(pg_con) => {
            if let PgConnectionType::PgConn(conn) = pg_con.conn {
                for table_name in TableName::get_all_table_names() {
                    delete_table(&mut *conn.lock().await, table_name).await?;
                }
            } else {
                log::error!("Unable to get internal tx connection");
            }
        }
        _ => {
            log::error!("Unable to get internal db connection");
        }
    }
    Ok(())
}

async fn delete_table(conn: &mut PooledConnection<ConnectionManager<PgConnection>>, table: TableName) -> Result<(), Error> {
    match table {
        TableName::Account => diesel::delete(account::table).execute(conn)?,
        TableName::BlockHead => diesel::delete(block_head::table).execute(conn)?,
        TableName::BlockHeader => diesel::delete(block_header::table).execute(conn)?,
        TableName::BlockMetaInfo => diesel::delete(block_meta_info::table).execute(conn)?,
        TableName::BlockProposer => diesel::delete(block_proposer::table).execute(conn)?,
        TableName::BlockTransaction => diesel::delete(block_transaction::table).execute(conn)?,
        TableName::Cluster => diesel::delete(cluster::table).execute(conn)?,
        TableName::Contract => diesel::delete(contract::table).execute(conn)?,
        TableName::ContractInstance => diesel::delete(contract_instance::table).execute(conn)?,
        TableName::ContractInstanceContractCodeMap => diesel::delete(contract_instance_contract_code_map::table).execute(conn)?,
        TableName::Event => diesel::delete(event::table).execute(conn)?,
        TableName::StakingAccount => diesel::delete(staking_account::table).execute(conn)?,
        TableName::StakingPool => diesel::delete(staking_pool::table).execute(conn)?,
        TableName::SubCluster => diesel::delete(sub_cluster::table).execute(conn)?,
        TableName::Validator => diesel::delete(validator_schema::table).execute(conn)?,
        TableName::Vote => diesel::delete(vote::table).execute(conn)?,
        TableName::VoteResult => diesel::delete(vote_result_schema::table).execute(conn)?,
        TableName::GenesisEpochBlock => diesel::delete(genesis_epoch_block::table).execute(conn)?,
    };
    Ok(())
}

pub async fn apply_db_dump(config: Config) -> Result<(), Error> {
    let snapshot_files = [DB_DUMP_SIGNATURE, DB_DUMP];

    let download_start_time = Instant::now();
    let mut download_last_error = None;
    
    // Try each node until we find one that serves all files
    for node in config.archive_snapshot_sources.clone() {
        let mut node_success = true;
        
        for file in snapshot_files {
            info!("Downloading file: {:?} from snapshot node: {:?}", file, node);
            match download_file(&node, file).await {
                Ok(_) => {
                    info!("Successfully downloaded {} from {}", file, node);
                },
                Err(e) => {
                    error!("Failed to download {} from {}: {}", file, node, e);
                    node_success = false;
                    download_last_error = Some(e);
                    break;  // Move to next node after first failure
                }
            }
        }
        
        if node_success {
            info!("All files downloaded successfully from {}", node);
            download_last_error = None;
            break;
        }
    }

    if let Some(e) = download_last_error {
        return Err(anyhow!(
            "Failed to download snapshot from all nodes. Last error: {} (from {})", 
            e,
            config.archive_snapshot_sources.last().unwrap_or(&"unknown".to_string())
        ));
    }

    info!(
        "⌛️ Files downloaded in: {:?} seconds",
        download_start_time.elapsed().as_secs()
    );
    if let Ok(_) = verify_signature(&config.archive_public_key).await {
        if let Err(error) = clear_db_records().await {
            return Err(anyhow!("Error while clearing db records: {:?}", error));
        }
        let restore_start_time = Instant::now();
        if let Db::Postgres { host, username, password, pool_size, db_name, test_db_name } = config.db {
            if let Err(error) = restore_db(
                username,
                password,
                host,
                db_name,
                DB_DUMP.to_string()
            ) {
                return Err(anyhow!("Error executing pg_restore command: {:?}", error));
            }
        } else {
            return Err(anyhow!("Invalid db config.(Required postgres config)"));
        }

        info!(
        "⌛️ Db dump restored in: {:?} seconds",
        restore_start_time.elapsed().as_secs()
    );
    } else {
        return Err(anyhow!("Invalid signature"));
    }
    Ok(())
}

async fn download_file(base_url: &str, file_name: &str) -> Result<(), Error> {
    // Create a Reqwest HTTP client
    let client = Client::new();
    let url = format!("{}{}", base_url, file_name);

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("user-agent","Wget/1.21.4".parse()?);
    
    info!("Start downloading {}", url);
    // Start the GET request
    let mut response = client.get(url).headers(headers).send().await?;
    // Ensure the request was successful
    if response.status().is_success() {
        // Open a file to write the stream to
        let mut file = File::create(file_name).await?;

        // Stream the response body and write it to the file chunk by chunk
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
        }
        info!("File downloaded successfully.");
    } else {
        error!("Download error: {}", response.status());
    }

    Ok(())
}

async fn verify_signature(pub_key: &str) -> Result<(), Error> {
    // Initialize Secp256k1 context
    let secp = Secp256k1::new();
    // Calculate hash of the large file
    let mut file = File::open("db_dump.dump").await.map_err(|e| {
        anyhow!("Failed to open file: {:?}", e)
    })?;
    let mut hasher = Sha256::new();
    let chunk_size = 1024 * 1024;

    info!("Started signature verification");

    // Read and hash the file in chunks
    loop {
        let mut chunk = vec![0; chunk_size];
        let bytes_read = file.read(&mut chunk).await.map_err(|e| {
            anyhow!("Failed to read file: {:?}", e)
        })?;
        if bytes_read == 0 {
            break; // End of file
        }
        hasher.update(&chunk[..bytes_read]);
    }

    let hash_result = hasher.finalize();
    let message = secp256k1::Message::from_digest_slice(&hash_result).map_err(|e| {
        anyhow!("Failed to create Message: {:?}", e)
    })?;

    // Read signature from file
    let mut signature_file = File::open("signature.bin").await.map_err(|e| {
        anyhow!("Failed to open signature file: {:?}", e)
    })?;
    let mut serialized_signature = [0; 64];
    signature_file.read_exact(&mut serialized_signature).await.map_err(|e| {
        anyhow!("Failed to read signature from file: {:?}", e)
    })?;
    // Parse the signature
    let signature = Signature::from_compact(&serialized_signature).map_err(|e| {
        anyhow!("Failed to parse signature: {:?}", e)
    })?;
    info!("Signature: {:?}", signature);
    // Create a PublicKey instance from the bytes
    let public_key = PublicKey::from_str(pub_key).map_err(|e| {
        anyhow!("Failed to get public key: {:?}", e)
    })?;

    // Verify signature
    let result = secp.verify_ecdsa(&message, &signature, &public_key);

    if result.is_ok() {
        info!("Signature verification successful!");
        Ok(())
    } else {
        Err(anyhow!("Signature verification failed!"))
    }
}

fn restore_db(username: String, password: String, host: String, db_name: String, dump_file_name: String) -> Result<(), Error> {
    info!("Restoring DB");
    // Specify the PostgreSQL connection URL
    let database_url = format!("postgres://{}:{}@{}/{}", username, password, host, db_name);

    // Execute the pg_restore command to restore the database
    match Command::new("pg_restore").args(&["--no-owner", "--dbname", &database_url, &dump_file_name]).output() {
        Ok(output) => {
            // Check if the command executed successfully
            if output.status.success() {
                info!("Database dump restored");
            } else {
                warn!("{}", String::from_utf8_lossy(&output.stderr));
            }
        }
        Err(err) => {
            warn!("Error executing pg_restore command: {}", err);
        }
    }
    Ok(())
}