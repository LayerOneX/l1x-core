use crate::sync::{multiaddrs_to_http_urls, sync_node};
use consensus::consensus::Consensus;
use db::db::{Database, DbTxConn};
use l1x_rpc::rpc_model::node_client::NodeClient;
use l1x_rpc::rpc_model::GetGenesisBlockRequest;
use l1x_vrf::common::SecpVRF;
use libp2p::identity::Keypair;
use log::{debug, error, info, warn};
use mempool::mempool::Mempool;
use node_info::node_info_state::NodeInfoState;
use p2p::network::{self, Event};
use primitives::*;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde_json::Value;
use std::{
	collections::HashMap, sync::Arc, thread, time::{Instant, SystemTime, UNIX_EPOCH}, usize
};
use std::str::FromStr;
use anyhow::Error;
use system::{
	account::Account, block::{Block, BlockBroadcast, BlockQueryRequest, BroadcastNodeDetailedStatus, L1xResponse, QueryBlockResponse}, block_proposer::BlockProposerBroadcast, config::MpscConfig, dht_health_storage::DHTHealthStorage, mempool::{ProcessMempool, ResponseMempool}, network::{BroadcastNetwork, EventBroadcast, NetworkMessage}, node_health::{AggregatedNodeHealthBroadcast, NodeHealthBroadcast}, node_info::{NodeInfoBroadcast, NodeInfoSignPayload}, transaction::TransactionBroadcast, vote::VoteBroadcast, vote_result::VoteResultBroadcast
};
use tokio::{
	sync::{broadcast, mpsc, Mutex},
	task,
	time::{sleep, Duration},
};
use structopt::StructOpt;
use system::{
	errors::NodeError,
	network::{BlockProposerEventType, NetworkAcknowledgement, NetworkEventType},
	transaction::Transaction,
	node_info::NodeInfo,
};
use block::{block_manager::BlockManager, block_state::BlockState};
use block_proposer::block_proposer_state::BlockProposerState;
use l1x_htm::realtime_checks::RealTimeChecks;
use system::config::InitialEpochConfig;
use system::block::QueryStatusRequest;
use system::node_health::NodeHealth;
use l1x_node_health::NodeHealthState;
use system::validator::Validator;
use validator::validator_state::ValidatorState;
use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct FullNode {
	pub ip_address: IpAddress,
	pub metadata: Metadata,
	pub node_address: Address,
	pub node_keypair: Keypair,
	pub cluster_address: Address,
	pub mempool_tx: mpsc::Sender<ProcessMempool>,
	pub node_event_tx: broadcast::Sender<EventData>,
	pub node_evm_event_tx: broadcast::Sender<EventBroadcast>,
	pub historical_sync_blocks: HashMap<BlockNumber, Block>,
}

#[derive(Debug, Clone)]
enum ProposeBlockStatus {
	Proposed(TimeStamp),
	NotProposed,
}

#[derive(Debug, PartialEq, StructOpt)]
pub enum NodeType {
	#[structopt(name = "full")]
	Full,
	#[structopt(name = "archive")]
	Archive,
}

impl FromStr for NodeType {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"full" => Ok(NodeType::Full),
			"archive" => Ok(NodeType::Archive),
			_ => Err(anyhow::anyhow!("Invalid node type: {}", s)),
		}
	}
}

impl <'a> FullNode {
	/// Create a new FullNode
	/// # Arguments
	/// * `max_size` - The maximum size of the mempool
	/// * `fee_limit` - The maximum fee that can be charged for a transaction
	/// * `rate_limit` - The maximum number of transactions that can be added to the mempool per
	///   second
	/// * `time_frame_seconds` - The time frame in seconds for the rate limit
	/// * `expiration_seconds` - The time in seconds for a transaction to expire
	/// * `dev_mode` - Whether or not to run the node in dev mode. Dev mode provides default dev
	///   accounts that are funded for use in testing
	/// * `multinode_mode` - Whether or not to run the node in multinode mode. Multinode mode allows
	///   the node to connect to other nodes in the network
	/// * `node_ip_address` - The ip address of the node
	/// * `node_keypair` - The keypair of the node
	/// * `bootnodes` - The bootnodes of the node
	pub async fn new(
		max_size: MemPoolSize,
		fee_limit: Balance,
		rate_limit: usize,
		time_frame_seconds: TimeStamp,
		expiration_seconds: TimeStamp,
		dev_mode: bool,
		multinode_mode: bool,
		node_ip_address: &str, // eg. "/ip4/0.0.0.0/tcp/5010"
		node_priv_key: Option<String>,
		bootnodes: &[&str], // er. &["/ip4/0.0.0.0/tcp/5010/p2p/1234567890"]
		autonat_config: &Option<system::config::AutonatConfig>,
		sync_batch_size: u16,
		block_time: TimeStamp,
		cluster_address: Address,
		validator_pool_address: Address,
		node_type: &NodeType,
		initial_epoch_config: Option<InitialEpochConfig>,
		eth_chain_id: Option<u64>,
		mpsc_channel_capacity: MpscConfig,
	) -> (FullNode, mpsc::Receiver<ResponseMempool>) {
		let server_info = format!(r#"
        _     __        _   _           _
        | |   /_ |      | \ | |         | |
        | |    | |_  __ |  \| | ___   __| | ___
        | |    | \ \/ / | . ` |/ _ \ / _` |/ _ \
        | |____| |>  <  | |\  | (_) | (_| |  __/
        |______|_/_/\_\ |_| \_|\___/ \__,_|\___|
          ____                   _                       _                _                                                          _
         / __ \                 (_)                     (_)              | |                                                        | |
        | |  | |_ __   ___ ___   _ _ __ ___   __ _  __ _ _ _ __   ___  __| |    _ __   _____      __  _ __  _ __ _____   _____ _ __ | |
        | |  | | '_ \ / __/ _ \ | | '_ ` _ \ / _` |/ _` | | '_ \ / _ \/ _` |   | '_ \ / _ \ \ /\ / / | '_ \| '__/ _ \ \ / / _ \ '_ \| |
        | |__| | | | | (_|  __/ | | | | | | | (_| | (_| | | | | |  __/ (_| |_  | | | | (_) \ V  V /  | |_) | | | (_) \ V /  __/ | | |_|
         \____/|_| |_|\___\___| |_|_| |_| |_|\__,_|\__, |_|_| |_|\___|\__,_( ) |_| |_|\___/ \_/\_/   | .__/|_|  \___/ \_/ \___|_| |_(_)
                                                    __/ |                  |/                        | |
                                                   |___/                                             |_|

             Version: {}

             Initializing...

            "#, env!("CARGO_PKG_VERSION"));
		info!("{}", server_info);

		let dev_mode_enabled = match dev_mode {
			true => "Enabled",
			false => "Disabled",
		};
		let multinode_mode_enabled = match multinode_mode {
			true => "Enabled",
			false => "Disabled",
		};
		info!("ℹ️ \n| 🛠️ DEV MODE: {}        |\n| 🌐 MULTINODE MODE: {} |\n", dev_mode_enabled, multinode_mode_enabled);

		debug!(" Default MPSCCapacity: {:?}", mpsc_channel_capacity);

		let node_private_key = match node_priv_key.clone() {
			Some(node_private_key) => node_private_key,
			None => {
				error!("🚨 Node - Security | Private Key not found");
				panic!()
			},
		};
		let secret_key = SecretKey::from_slice(
			&hex::decode(&node_private_key).expect("🚨 Node - Security | Error decoding node_private_key"),
		)
		.expect("🚨 Node - Security | Failed to parse provided private_key");
		let secp = Secp256k1::new();
		let verifying_key = secret_key.public_key(&secp);
		let verifying_key_bytes = verifying_key.serialize().to_vec();
		let node_address =
			Account::address(&verifying_key_bytes).expect("🚨 Node - Security | Failed to get node address");

		info!("🔍 Node - Security | Address: 0x{}, Verifying key: {:?}", hex::encode(node_address), hex::encode(&verifying_key_bytes));

		let node_keypair =  {
				let mut keypair_bytes =
					hex::decode(&node_private_key).expect("🚨 Node - Security | Failed to hex decode privkey");
				let secret_key =
					libp2p::identity::secp256k1::SecretKey::try_from_bytes(&mut keypair_bytes)
						.expect("🚨 Node - Security | Failed to parse keypair");

				// Create a new Keypair using the secp256k1::Keypair constructor
				let secp_keypair = libp2p::identity::secp256k1::Keypair::from(secret_key);

				// Use try_into_secp256k1 from libp2p_identity to convert to Keypair
				let keypair: libp2p::identity::Keypair =
					secp_keypair.try_into().expect("🚨 Node - Security | Failed to convert to Keypair");

				keypair
			};
		// initialize database
		let db_pool_conn =
			Database::get_pool_connection().await.expect("🚨 Node - Database | Unable to get db_pool_conn");

		let (mempool_res_tx, mempool_res_rx) = mpsc::channel(mpsc_channel_capacity.mempool_res);
		let (mempool_tx, mempool_rx) = mpsc::channel(mpsc_channel_capacity.mempool);
		let (network_client_tx, network_client_rx) = mpsc::channel(mpsc_channel_capacity.network_client);
		let (network_receive_tx, network_receive_rx) = mpsc::channel(mpsc_channel_capacity.network_receive);
		let (timer_tx, timer_rx) = mpsc::channel(mpsc_channel_capacity.timer);
		// EVM (and maybe L1XVM) events are written to the sender channel.
		// let (event_tx, event_rx) = mpsc::channel(1000);
		let (event_tx, _) = broadcast::channel(mpsc_channel_capacity.event);
		// For L1XVM events
		let (node_event_tx, _) = broadcast::channel(mpsc_channel_capacity.node_event);

		let ip_address = "127.0.0.1".as_bytes().to_vec();
		let metadata = "metadata".as_bytes().to_vec();
		//node peer_id
		let peer_id = node_keypair.clone().public().to_peer_id();
		let dht_health_storage = DHTHealthStorage::new();
		let node_info_state = NodeInfoState::new(&db_pool_conn)
			.await
			.expect("🚨 Node - Database | Unable to get node info state");
		let block_manager = BlockManager{};
		let block_state = BlockState::new(&db_pool_conn).await.expect("🚨 Node - Database | failed to load block state");

		if *node_type == NodeType::Full {
			info!("🚀 Node - Starting full node");
			let (mut network_client, event_receiver, event_loop) =
				network::new(node_keypair.clone(), bootnodes, dht_health_storage, autonat_config, eth_chain_id, Some(hex::encode(cluster_address.clone())), mpsc_channel_capacity.clone())
					.await
					.expect("🚨 Node - Network | Network to be created");

			// Spawn the networking event loop
			tokio::spawn(event_loop.run());

			// Start listening
			network_client
				.start_listening(node_ip_address.parse().expect("🚨 Node - Network | Address parses correctly"))
				.await
				.expect("🚨 Node - Network | Listening not to fail");

			// Start the node listening for supported events from the networking code for the node to
			// handle
			task::spawn(Self::event_receiver_process(
				event_receiver,
				mempool_tx.clone(),
				network_receive_tx.clone(),
			));

			// Handling EVM events, but can be updated to handle L1XVM events as well in the future I
			// think
			task::spawn(Self::evm_events(event_tx.clone()));

			// Handling L1XVM events
			task::spawn(Self::node_event_receiver_process(node_event_tx.clone()));

			match try_initialize_runtime_configs(cluster_address).await {
				Ok(_) => info!("🔍 Node - Runtime | Configs initialized, attempt 1"),
				Err(e) => error!("🚨 Node - Runtime | Failed to initialize runtime config in attempt 1: {}", e),
			}

			// Only sync when in multinode mode
			if multinode_mode && !bootnodes.is_empty() {
				let sync_start_time = Instant::now();
				match sync_node(cluster_address, bootnodes, event_tx.clone(), 500, mpsc_channel_capacity.block_batch).await {
					Ok(_) => {
						info!("🔍 Node - Syncing | Node synced successfully ✅");
						info!(
						"⌛️ Syncing node took: {:?} seconds",
						sync_start_time.elapsed().as_secs()
					);
					},
					Err(e) => {
						panic!("🚨 Node - Syncing | Unable to sync node: {:?}", e);
					},
				}

				// Update node info
				match update_node_info(bootnodes).await {
					Ok(_) => info!("🔍 Node - Syncing | Node info sync successful ✅"),
					Err(e) => error!("🚨 Node - Syncing | Unable to sync node info: {:?}", e),
				}

				// Update genesis block
				match update_genesis_block(bootnodes).await {
					Ok(_) => info!("🔍 Node - Syncing | Genesis block updated successfully ✅"),
					Err(e) => warn!("🚨 Node - Syncing | Failed to update genesis block: {:?}", e),
				}

				// add sleep
				let duration = Duration::from_secs(5);
				thread::sleep(duration);

				// Sometimes few last blocks are missed:
				// they are not synced on the prevoius step and not cached by p2p
				info!("🔍 Node - Syncing | Sync node one more time");
				let sync_start_time = Instant::now();
				match sync_node(cluster_address, bootnodes, event_tx.clone(), 500, mpsc_channel_capacity.block_batch).await {
					Ok(_) => {
						info!("🔍 Node - Syncing | Syncing node successful ✅");
						info!(
						"🕒 Node - Syncing | Syncing node took: {:?} seconds",
						sync_start_time.elapsed().as_secs()
					);
					},
					Err(e) => {
						panic!("🚨 Node - Syncing | Unable to sync node: {:?}", e);
					},
				}

				// Initialize runtime configs, one more time to make sure it's initialized
				match try_initialize_runtime_configs(cluster_address).await {
					Ok(_) => info!("🔍 Node - Runtime | Configs initialized, attempt 2"),
					Err(e) => error!("🚨 Node - Runtime | Failed to initialize runtime config in attempt 2: {}", e),
				}
			}

			let current_block_height = block_state.block_head_header(cluster_address).await.expect("🚨 Node - Database | failed to get block height").block_number;
			let current_epoch = block_manager.calculate_current_epoch(current_block_height).expect("🚨 Node - Database | failed to current calculate epoch") as u64;
			let maybe_new_epoch = block_manager.calculate_current_epoch(current_block_height + 1).expect("🚨 Node - Database | failed to current calculate epoch") as u64;
			let latest_epoch = current_epoch.max(maybe_new_epoch);

			// Load node info and create new if not exist.
			let node_info = if let Ok(node_info) = node_info_state.load_node_info(&node_address).await {
				if node_info.peer_id.is_empty() {
					let mut updated_node_info = node_info.clone();
					updated_node_info.peer_id = peer_id.to_string();
					match node_info_state.update_node_info(&node_info).await {
						Ok(_) => {
							info!("🔍 Node - Database | Node info updated");
						}
						Err(e) => {
							warn!("🚨 Node - Database | failed to update node info because of {}", e);
						}
					};
					updated_node_info
				} else {
					node_info
				}
			} else {
				let node_info_payload = NodeInfoSignPayload {
					ip_address: "127.0.0.1".as_bytes().to_vec(),
					metadata: "metadata".as_bytes().to_vec(),
					cluster_address: cluster_address.clone(),
				};

				let signature = node_info_payload
					.sign_with_ecdsa(secret_key)
					.expect("🚨 Node - Database | Unable to sign node_info_payload");

				let node_info = NodeInfo::new(
					node_address.clone(),
					peer_id.to_string(),
					latest_epoch,
					"127.0.0.1".as_bytes().to_vec(),
					"metadata".as_bytes().to_vec(),
					cluster_address.clone(),
					signature.serialize_compact().to_vec(),
					verifying_key_bytes.clone(),
				);

				match node_info_state.store_node_info(&node_info).await {
					Ok(_) => {
						info!("🔍 Node - Database | Stored node info");
					}
					Err(e) => {
						warn!("🚨 Node - Database | failed to store node info because of {}", e);
					}
				};
				node_info
			};

			let real_time_checks = RealTimeChecks::new();

			let consensus = Consensus::new(
				event_tx.clone(),
				node_event_tx.clone(),
				network_client_tx.clone(),
				mempool_tx.clone(),
				node_address.clone(),
				validator_pool_address.clone(),
				cluster_address.clone(),
				secret_key.clone(),
				verifying_key.clone(),
				multinode_mode,
				node_info.clone(),
				real_time_checks,
			);

			// Create a fresh mempool
			let mempool = Mempool::new(
				max_size,
				fee_limit,
				rate_limit,
				time_frame_seconds,
				expiration_seconds,
				network_client_tx.clone(),
				event_tx.clone(),
				cluster_address.clone(),
				node_address.clone(),
				secret_key.clone(),
				verifying_key.clone(),
				multinode_mode,
			);

			let full_node = FullNode {
				ip_address: ip_address.clone(),
				metadata: metadata.clone(),
				node_address: node_address.clone(),
				node_keypair: node_keypair.clone(),
				cluster_address: cluster_address.clone(),
				mempool_tx: mempool_tx.clone(),
				node_event_tx: node_event_tx.clone(),
				node_evm_event_tx: event_tx.clone(),
				historical_sync_blocks: HashMap::new(),
			};

			task::spawn(Self::block_production_timer(
				mempool_tx,
				timer_rx,
				network_receive_tx.clone(),
				block_time,
				multinode_mode,
			));

			task::spawn(Self::process_mempool(
				mempool_rx,
				mempool_res_tx,
				timer_tx,
				node_event_tx.clone(),
				mempool,
				secret_key,
				verifying_key,
				network_receive_tx.clone(),
			));

			task::spawn(Self::broadcast_network(
				network_client_rx,
				network_client.clone(),
				secret_key,
				verifying_key,
			));

			task::spawn(Self::receive_network(network_receive_rx, consensus));

			if multinode_mode {
				if !bootnodes.is_empty() {
					if latest_epoch == 0 {
						// setting bootnode as initial block proposer
						let initial_block_proposer_address = initial_epoch_config
							.expect("🚨 Node - Database | Initial epoch config not provided")
							.block_proposer
							.as_ref()
							.map(|proposer| hex::decode(proposer).expect("🚨 Node - Database | Unable to decode initial_block_proposer string"))
							.and_then(|bytes| bytes.try_into().ok())
							.expect("🚨 Node - Database | Initial block proposer not provided or wrong length of Vec");

						set_initial_block_proposer(cluster_address, initial_block_proposer_address, &db_pool_conn, current_epoch).await;
					} else {
						// Update node health for current epoch
						update_node_healths(bootnodes, current_epoch)
							.await
							.unwrap_or_else(|err| panic!("🚨 Node - Syncing | Unable to sync node health for epoch {}: {:?}", current_epoch, err));

						info!("🔍 Node - Syncing | Node health sync successful ✅");
						// Update block_proposer and validators for current epoch
						update_block_proposer_and_validator(bootnodes, latest_epoch, cluster_address)
							.await
							.unwrap_or_else(|err| panic!("🚨 Node - Syncing | Unable to sync Block proposer and validators for epoch {}: {:?}", latest_epoch, err));
						info!("🔍 Node - Syncing | Block proposer and validators sync successful ✅");
					}
				} else {
					block_state
						.store_genesis_block_time(current_block_height, latest_epoch)
						.await
						.expect("🚨 Node - Database | failed to store genesis block");

					// setting bootnode as initial block proposer
					let block_proposer_state = BlockProposerState::new(&db_pool_conn)
						.await
						.expect("🚨 Node - Database | failed to get block_proposer_state");
					block_proposer_state.upsert_block_proposer(cluster_address, latest_epoch, node_address)
						.await
						.expect("🚨 Node - Database | failed to set initial block proposer");
				}

				// Give time for the node to connect to a peer before broadcasting its node info
				let duration = Duration::from_secs(2);
				thread::sleep(duration);

				// Broadcast the nodes info to the network on startup
				broadcast_node_info(&node_address, &db_pool_conn, network_client_tx).await;

				// Start node health monitoring
				network_client.init_node_monitoring(
					cluster_address.clone(),
					latest_epoch,
					peer_id.clone(),
				).await.expect("🚨 Node - Network | Failed to start node health monitoring");

			
			}

			(full_node, mempool_res_rx)
		} else {
			info!("🔍 Node - Starting archive node 📦");
			info!("🔍 Node - Try to initialize runtime configs");
			match try_initialize_runtime_configs(cluster_address).await {
				Ok(_) => info!("🔍 Node - Runtime | Runtime configs initialized"),
				Err(e) => error!("🚨 Node - Runtime | Failed to initialize runtime configs: {}", e),
			}

			
			let full_node = FullNode {
				ip_address: ip_address.clone(),
				metadata: metadata.clone(),
				node_address: node_address.clone(),
				node_keypair,
				cluster_address: cluster_address.clone(),
				mempool_tx: mempool_tx.clone(),
				node_event_tx: node_event_tx.clone(),
				node_evm_event_tx: event_tx.clone(),
				historical_sync_blocks: HashMap::new(),
			};
			(full_node, mempool_res_rx)
		}
	}

	async fn evm_events(node_event_tx: broadcast::Sender<EventBroadcast>) {
		// loop {
		//     match event_receiver.recv().await {
		//         Some(event) => {
		//             info!("NEW EVENT: {event:?}");
		//         }
		//         None => {
		//             warn!("Shouldn't happen?");
		//         }
		//     }
		// }

		// Handle incoming node events
		let mut node_event_rx = node_event_tx.subscribe();

		loop {
			match node_event_rx.recv().await {
				Ok(event) => {
					// publish to WS event topic
					info!(
						"🔍 Node - EVM | Received EVM event: {event:?}",
						/* serde_json::from_slice::<Value>(&event)
						 * .map(|x| x.to_string())
						 * .unwrap_or_else(|_| format!("Unserializable event: {event:?}")) */
					);
				},
				Err(e) => {
					warn!("🚨 Node - EVM | Unable to read event from event_tx channel: {:?}", e);
				},
			}
		}
	}

	async fn node_event_receiver_process(node_event_tx: broadcast::Sender<EventData>) {
		// Handle incoming node events
		let mut node_event_rx = node_event_tx.subscribe();

		loop {
			match node_event_rx.recv().await {
				Ok(event) => {
					// publish to WS event topic
					info!(
						"🔍 Node - Event | Received node event: {}",
						serde_json::from_slice::<Value>(&event)
							.map(|x| x.to_string())
							.unwrap_or_else(|_| format!("🚨 Node - Event | Unserializable event: {event:?}"))
					);
				},
				Err(e) => {
					warn!("🚨 Node - Event | Unable to read event from event_tx channel: {:?}", e);
				},
			}
		}
	}

	/// When the node receives new supported events over the p2p network, they are routed here for
	/// further handling
	async fn event_receiver_process(
		mut event_receiver: mpsc::Receiver<Event>,
		mempool_tx: mpsc::Sender<ProcessMempool>,
		network_receive_tx: mpsc::Sender<NetworkMessage>,
	) {
		debug!("🔍 Node - Event | Event receiver process started");
		// Handle incoming network events
		loop {
			match event_receiver.recv().await {
				Some(event) => {
					match event {
						// A transaction has just been received from another node in the network.
						// Add it to this nodes mempool if it is valid.
						Event::InboundTransaction { transaction } => {
							info!("🔍 Node - Event | I just received a new TX from the network 📨");
							if let Err(e) =
								mempool_add_transaction(mempool_tx.clone(), transaction).await	{
								warn!("🚨 Node - Event | Unable to write transaction to mempool channel: {:?}", e)
							}
						},
						Event::InboundNodeInfo { node_info } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveNodeInfo(node_info)))
								.await
							{
								warn!(
									"🚨 Node - Event | Unable to write node_info to network_receive_tx channel: {:?}",
									e
								)
							}
						},
						Event::InboundValidateBlock { block_payload } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveValidateBlock(block_payload)))
								.await
							{
								warn!("🚨 Node - Event | Unable to write block_payload to network_receive_tx channel: {:?}", e)
							}
						},
						Event::InboundBlockProposer { block_proposer_payload } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveBlockProposer(block_proposer_payload)))
								.await
							{
								warn!("🚨 Node - Event | Unable to write block_payload to network_receive_tx channel: {:?}", e)
							}
						},
						Event::InboundVote { vote } => {
							if let Err(e) =
								network_receive_tx.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveVote(vote))).await
							{
								warn!("🚨 Node - Event | Unable to write vote to network_receive_tx channel: {:?}", e)
							}
						},
						Event::InboundVoteResult { vote_result } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveVoteResult(vote_result)))
								.await
							{
								warn!("🚨 Node - Event | Unable to write vote_result to network_receive_tx channel: {:?}", e)
							}
						},

						Event::InboundQueryBlockResponse{ block_payload, is_finalized, vote_result } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveQueryBlockResponse(block_payload, vote_result)))
								.await
							{
								warn!("🚨 Node - Event | Unable to write response to network_receive_tx channel: {:?}", e)
							}
						},
						Event::InboundQueryBlockRequest { request, channel } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveQueryBlockRequest(request, channel)))
								.await
							{
								warn!("🚨 Node - Event | Unable to write response to network_receive_tx channel: {:?}", e)
							}
						},
						Event::ProcessNodeHealth { epoch } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ProcessNodeHealth(epoch)))
								.await
							{
								warn!("🚨 Node - Event | Unable to send ReceiveNodeHealth event: {:?}", e);
							}
						},
						Event::InboundNodeHealth{node_healths} => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveNodeHealth(node_healths)))
								.await
							{
								warn!("🚨 Node - Event | Unable to send ReceiveNodeHealth event: {:?}", e);
							}
						},
						Event::InboundAggregatedNodeHealth{aggregated_healths} => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveAggregatedNodeHealth(aggregated_healths)))
								.await
							{
								warn!("🚨 Node - Event | Unable to send ReceiveAggregatedNodeHealth event: {:?}", e);
							}
						},
						Event::AggregateNodeHealth { epoch } => {
							debug!("🔍 Node - Event | Sending Event::AggregateNodeHealth event to network_receive_tx channel, epoch: {}", epoch);
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::AggregateNodeHealth(epoch)))
								.await
							{
								warn!("🚨 Node - Event | Unable to send Aggregate NodeHealth event: {:?}", e);
							}
						},
						Event::PingResult { peer_id, is_success, rtt } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::PingResult(peer_id, is_success, rtt)))
								.await
							{
								warn!("🚨 Node - Event | Unable to handler ping result event: {:?}", e);
							}
						},
						Event::PingEligiblePeers { epoch, peer_ids } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::PingEligiblePeers(epoch, peer_ids)))
								.await
							{
								warn!("🚨 Node - Event | Unable to handler ping result event: {:?}", e);
							}
						},
						Event::CheckNodeStatus => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::CheckNodeStatus))
								.await
							{
								warn!("🚨 Node - Event | Unable to send check node status event: {:?}", e);
							}
						},
						Event::InitializePeers => {
							debug!("🔍 Node - Event | Received Event::InitializePeers event");
						},
						Event::PublishNodeDetailedStatus { peer_id } => {
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::PublishNodeDetailedStatus(peer_id)))
								.await
							{
								warn!("🚨 Node - Event | Unable to write detailed status request to network_receive_tx channel: {:?}", e)
							}
						},
						Event::InboundNodeDetailedStatus(node_detailed_status) => {
							debug!("🔍 Node - Event | Received Event::InboundNodeDetailedStatus event: {:?}", node_detailed_status.clone());
							if let Err(e) = network_receive_tx
								.send(NetworkMessage::NetworkEvent(NetworkEventType::ReceiveNodeDetailedStatus(node_detailed_status)))
								.await
							{
								warn!("🚨 Node - Event | Unable to write detailed status response to network_receive_tx channel: {:?}", e)
							}
						}
					}
				},
				// Command channel closed, thus shutting down the network event loop.
				None => {
					warn!("🚨 Node - Event | Event receiver process worker exited");
					return
				}
			}
		}
	}

	async fn block_production_timer(
		mempool_tx: mpsc::Sender<ProcessMempool>,
		mut timer_rx: mpsc::Receiver<ProposeBlockStatus>,
		network_sender_tx: mpsc::Sender<NetworkMessage>,
		block_time: TimeStamp, // Assuming block_time is in milliseconds
		multinode_mode: bool,
	)
	{

		debug!("🔍 Node - Block Production | Inside block_production_timer: started, block_time: {}", block_time);

		let mut last_propose_block_time = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("🚨 Node - Block Production | Failed to get system time")
			.as_millis();

		loop {
			sleep(Duration::from_millis(1000)).await;
			let current_time = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("🚨 Node - Block Production | Failed to get system time")
				.as_millis();

			if (last_propose_block_time + block_time) <= current_time {
				if let Err(e) = mempool_tx.send(ProcessMempool::ProposeBlockOnBlockTime).await {
					warn!("🚨 Node - Block Production | Unable to write time to mempool_tx channel: {:?}", e);
				}
			} else {
				if let Err(e) = mempool_tx.send(ProcessMempool::ProposeBlockOnMempoolFull).await {
					warn!("🚨 Node - Block Production | Unable to write time to mempool_tx channel: {:?}", e);
				}
			}

			match timer_rx.recv().await {
				Some(propose_block_status) =>  {
					if let ProposeBlockStatus::Proposed(propose_block_time) = propose_block_status {
						last_propose_block_time = propose_block_time;
					}
				}
				None => warn!("🚨 Node - Block Production | timer_rx channel closed"),
			}
		}
	}
	async fn process_mempool(
		mut mempool_rx: mpsc::Receiver<ProcessMempool>,
		mempool_res_tx: mpsc::Sender<ResponseMempool>,
		timer_tx: mpsc::Sender<ProposeBlockStatus>,
		node_event_tx: broadcast::Sender<EventData>,
		mut mempool: Mempool,
		secret_key: SecretKey,
		verifying_key: PublicKey,
		mut network_receive_tx: mpsc::Sender<NetworkMessage>,
	){
		let db_pool_conn =
			Database::get_pool_connection().await.expect("🚨 Node - Block Production | error getting db_pool_conn");

		while let Some(process_mempool) = mempool_rx.recv().await {
			match process_mempool {
				ProcessMempool::AddTransaction(transaction, sender) => {
					match mempool.remove_expired_transactions().await {
						Ok(expired_transactions) => {
							for expired_transaction in expired_transactions {
								let tx_hash_string = expired_transaction.transaction_hash()
									.ok()
									.and_then(|v| Some(hex::encode(&v)));
								info!("🔍 Node - Block Production | Expired transaction: tx hash: {:?} & is_eth_tx: {:?}",
									tx_hash_string,
									expired_transaction.eth_original_transaction.is_some());
							}
						},
						Err(error) => error!("🚨 Node - Block Production | Error while clearing expired transactions: {:?}", error),
					}
					match mempool.add_transaction(transaction, &db_pool_conn).await {
						Ok(_) => {
							if let Err(e) = sender.send(Ok(ResponseMempool::Success)) {
								error!("🚨 Node - Block Production | Unable to write ResponseMempool to mempool_res_tx channel: {:?}", e);
							}
							if let Err(e) = mempool_res_tx.send(ResponseMempool::Success).await {
								error!("🚨 Node - Block Production | Unable to write ResponseMempool to mempool_res_tx channel: {:?}", e);
							}
						},
						Err(e) => {
							if let Err(e) = sender.send(Ok(ResponseMempool::FailedToAddTransaction)) {
								error!("🚨 Node - Block Production | Unable to write ResponseMempool to mempool_res_tx channel: {:?}", e);
							}
							if let Err(e) =
								mempool_res_tx.send(ResponseMempool::FailedToAddTransaction).await
							{
								error!("🚨 Node - Block Production | Unable to write ResponseMempool to mempool_res_tx channel: {:?}", e);
							}
							warn!("🚨 Node - Block Production | Unable to add transaction to mempool: {:?}", e);
						},
					};
				},
				ProcessMempool::RemoveTrasaction(transaction) => {
					match mempool.remove_transaction(&transaction).await {
						Ok(_) => (),
						Err(e) => warn!("⚠️ Node - Block Production | Remove trasaction from mempool failed: {e:?}"),
					}
				},
				ProcessMempool::ProposeBlockOnMempoolFull => {
					match mempool.remove_expired_transactions().await {
						Ok(expired_transactions) => {
							for expired_transaction in expired_transactions {
								let tx_hash_string = expired_transaction.transaction_hash()
									.ok()
									.and_then(|v| Some(hex::encode(&v)));
								info!("🔍 Node - Block Production | Expired transaction: tx hash: {:?} & is_eth_tx: {:?}",
									tx_hash_string,
									expired_transaction.eth_original_transaction.is_some());
							}
						},
						Err(error) => error!("🚨 Node - Block Production | Error while clearing expired transactions: {:?}", error),
					}
					if mempool.transactions_priority.len() >= mempool.max_size {
						info!("🔍 Node - Block Production | ProposeBlockOnMempoolFull event");
						match mempool.propose_block(&db_pool_conn, secret_key, verifying_key, network_receive_tx.clone()).await {
							Ok(result) => {
								match result {
									mempool::mempool::ProposeBlockResult::Proposed(events) => {
										for event in events {
											if let Err(err) = node_event_tx.send(event) {
												warn!(
													"🚨 Node - Block Production | Failed to publish ProposeBlock event due to {:?}",
													err
												);
											}
										}
										let current_timestamp = SystemTime::now()
											.duration_since(UNIX_EPOCH)
											.expect("🚨 Node - Block Production | Failed to get system time")
											.as_millis();
										if let Err(e) = timer_tx
											.send(
												ProposeBlockStatus::Proposed(current_timestamp)
											)
											.await
										{
											error!("🚨 Node - Block Production | Unable to write time to timer_tx channel: {:?}", e)
										}
									},
									mempool::mempool::ProposeBlockResult::NotProposed => {
										if let Err(e) = timer_tx
											.send(
												ProposeBlockStatus::NotProposed
											)
											.await
										{
											error!("🚨 Node - Block Production | Unable to write time to timer_tx channel: {:?}", e)
										}
									}
								}
							},
							Err(e) => {
								error!(
									"🚨 Node - Block Production | Propose block failed on ProposeBlockOnMempoolFull: {:?}",
									e
								);
								if let Err(e) = timer_tx
									.send(
										ProposeBlockStatus::NotProposed
									)
									.await
								{
									error!("🚨 Node - Block Production | Unable to write time to timer_tx channel: {:?}", e)
								}
							},
						};
					} else {
						if let Err(e) = timer_tx
							.send(
								ProposeBlockStatus::NotProposed
							)
							.await
						{
							error!("🚨 Node - Block Production | Unable to write time to timer_tx channel: {:?}", e)
						}
					}
				},
				ProcessMempool::ProposeBlockOnBlockTime => {
					match mempool.remove_expired_transactions().await {
						Ok(expired_transactions) => {
							for expired_transaction in expired_transactions {
								let tx_hash_string = expired_transaction.transaction_hash()
									.ok()
									.and_then(|v| Some(hex::encode(&v)));
								info!("🔍 Node - Block Production | Expired transaction: tx hash: {:?} & is_eth_tx: {:?}",
									tx_hash_string,
									expired_transaction.eth_original_transaction.is_some());
							}
						},
						Err(error) => error!("🚨 Node - Block Production | Error while clearing expired transactions: {:?}", error),
					}
					match mempool.propose_block(&db_pool_conn, secret_key, verifying_key, network_receive_tx.clone()).await {
						Ok(result) => {
							match result {
								mempool::mempool::ProposeBlockResult::Proposed(events) => {
									for event in events {
										if let Err(err) = node_event_tx.send(event) {
											warn!("⚠️ Node - Block Production | Failed to publish ReceiveBlock event due to {:?}", err);
										}
									}
									let current_timestamp = SystemTime::now()
										.duration_since(UNIX_EPOCH)
										.expect("🚨 Node - Block Production | Failed to get system time")
										.as_millis();
									if let Err(e) = timer_tx
										.send(
											ProposeBlockStatus::Proposed(current_timestamp)
										)
										.await
									{
										error!("🚨 Node - Block Production | Unable to write time to timer_tx channel: {:?}", e)
									}
								},
								mempool::mempool::ProposeBlockResult::NotProposed => {
									if let Err(e) = timer_tx
										.send(
											ProposeBlockStatus::NotProposed
										)
										.await
									{
										error!("🚨 Node - Block Production | Unable to write time to timer_tx channel: {:?}", e)
									}
								}
							}
						},
						Err(e) => {
							error!("🚨 Node - Block Production | Propose block failed on ProposeBlockOnBlockTime: {:?}", e);
							if let Err(e) = timer_tx
								.send(
									ProposeBlockStatus::NotProposed
								)
								.await
							{
								error!("🚨 Node - Block Production | Unable to write time to timer_tx channel: {:?}", e)
							}
						},
					};
				},
				ProcessMempool::GetSize(sender) => {
					if let Err(e) = sender.send(mempool.transactions_priority.len()) {
						error!("🚨 Node - Block Production | Unable to write size to mempool_res_tx channel: {:?}", e);
					}
				}
			}
		}

		warn!("⚠️ Node - Block Production | process_mempool worker exited");
	}

	async fn broadcast_network(
		mut network_client_rx: mpsc::Receiver<BroadcastNetwork>,
		mut network_client: network::Client,
		secret_key: SecretKey,
		verifying_key: PublicKey,
	) {
		while let Some(broadcast_network) = network_client_rx.recv().await {
			match broadcast_network {
				BroadcastNetwork::BroadcastNodeInfo(node_info) => {
					match network_client.node_info_broadcast(node_info).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!(
										"⚠️ Node - Block Production | Unable to broadcast node_info using network_client: {:?}",
										e
									);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastTransaction(transaction) => {
					match network_client.transaction_broadcast(transaction).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast transaction using network_client: {:?}", e);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastValidateBlock(block_payload) => {
					match network_client.block_validate_broadcast(block_payload).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast validate_block using network_client: {:?}", e);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastBlockProposer(block_proposer_payload) => {
					/*let json_str = serde_json::to_string(&cluster_block_proposers)
						.expect("Unable to parse to json string");
					let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());
					let sig = secret_key.sign_ecdsa(message);
					let block_proposer_payload = BlockProposerPayload {
						cluster_block_proposers,
						signature: sig.serialize_compact().to_vec(),
						verifying_key: verifying_key.serialize().to_vec(),
					};*/
					match network_client.block_proposer_broadcast(block_proposer_payload).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!(
										"⚠️ Node - Block Production | Unable to broadcast block_proposer using network_client: {:?}",
										e
									);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastVote(vote) => {
					match network_client.vote_broadcast(vote).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast vote using network_client: {:?}", e);
								},
							}
						},
					};
				},

				BroadcastNetwork::BroadcastVoteResult(vote_result) => {
					match network_client.vote_result_broadcast(vote_result).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast vote_result using network_client: {:?}", e);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastQueryBlockRequest(request, peer) => {
					match network_client.block_query_request(request, peer).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast block request using network_client: {:?}", e);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastQueryBlockResponse(response, channel ) => {
					match network_client.block_query_response(response, channel).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast block request using network_client: {:?}", e);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastNodeHealth(node_healths) => {
					match network_client.node_health_broadcast(node_healths).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast node health: {:?}", e);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastSignedNodeHealth(aggregated_healths) => {
					match network_client.aggregated_node_health_broadcast(aggregated_healths).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast aggregated node health: {:?}", e);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastQueryNodeStatusRequest(request_time, peer_id) => {
					match network_client.query_status_request(request_time, peer_id).await {
						Ok(_) => {},
						Err(e) => {
							match e.to_string().as_str() {
								"Duplicate" => {
									// Ignore this warning
								},
								_ => {
									warn!("⚠️ Node - Block Production | Unable to broadcast query node status request: {:?}", e);
								},
							}
						},
					};
				},
				BroadcastNetwork::BroadcastNodeDetailedStatus(node_detailed_status) => {
					match network_client.broadcast_node_detailed_status(node_detailed_status).await {
						Ok(_) => {},
						Err(e) => {
							warn!("⚠️ Node - Block Production | Unable to broadcast node detailed status: {:?}", e);
						},
					};
				},
			}
		}

		warn!("⚠️ Node - Block Production | Broadcast Network worker exited");
	}

	/// Routes the payloads received from the network to the consensus function
	async fn receive_network(
		mut network_receive_rx: mpsc::Receiver<NetworkMessage>,
		mut consensus: Consensus,
	){
		let db_pool_conn =
		Database::get_pool_connection().await.expect("🚨 Node - Receive Network | error getting db_pool_conn");

		while let Some(receive_network) = network_receive_rx.recv().await {
			match receive_network {
				NetworkMessage::NetworkEvent(network_event_type) => {
					FullNode::handle_network_event_type(network_event_type, &mut consensus, &db_pool_conn).await;
				},
				NetworkMessage::BlockProposerEvent(block_proposer_event_type) => {
					FullNode::handle_block_proposer_event_type(block_proposer_event_type, &mut consensus,  &db_pool_conn).await;
				},
			}
		}

		warn!("⚠️ Node - Receive Network | Receive Network worker exited");
	}

	async fn handle_block_proposer_event_type(payload: BlockProposerEventType, consensus: &mut Consensus, db_pool_conn: &'a DbTxConn<'a>,){
		match payload {
			BlockProposerEventType::AddBlock(block_payload, sender) => {
				info!("🔍 ℹ️ 🔗 Node - Receive Network | Handle Block Proposer Event Type | I just received a new Block #{} to be added into pending state ", block_payload.block.block_header.block_number);
				match consensus.add_and_broadcast_block(block_payload).await {
					Ok(_) => {
						// Operation was successful, send success acknowledgement
						if let Err(e) = sender
							.send(Ok(NetworkAcknowledgement::Success))
						{
							warn!("🚨 Node - Receive Network | Handle Block Proposer Event Type | Unable to write acknowledgement to network_receive_tx_ack channel: {:?}", e)
						}
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Block Proposer Event Type | Add and broadcast block failed: {:?}", e);
						// Send the failure acknowledgment
						if let Err(e) = sender.send(Ok(NetworkAcknowledgement::Failure))
						{
							warn!("🚨 Node - Receive Network | Handle Block Proposer Event Type | Unable to write acknowledgement to network_receive_tx_ack channel: {:?}", e)
						}
					},
				};
			},
		}
	}
	async fn handle_network_event_type(payload: NetworkEventType, consensus: &mut Consensus, db_pool_conn: &'a DbTxConn<'a>){
		match payload {
			NetworkEventType::ReceiveNodeInfo(node_info) => {
				info!("🔍 ℹ️ 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Node Info from the network | From: {}", hex::encode(&node_info.address));
				match consensus.receive_node_info(node_info, &db_pool_conn).await {
					Ok(_res) => {},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Failed to receive node info: {:?}", e);
					},
				};
			},
			NetworkEventType::ReceiveValidateBlock(block_payload) => {
				let block_number = block_payload.block.block_header.block_number;
				info!(
					"🔍 ⚖️ 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Validate Block #{} from the network ",
					block_number
				);
				match consensus.receive_validate_block(block_payload, &db_pool_conn).await {
					Ok(_res) => {},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Received block_payload failed validation, block #{}: {:?}", block_number, e);
					},
				};
			},
			NetworkEventType::ReceiveBlockProposer(block_proposer_payload) => {
				info!("🔍 👑 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Block Proposer from the network , From : {}", hex::encode(&block_proposer_payload.sender));
				match consensus.receive_block_proposer(block_proposer_payload, &db_pool_conn).await {
					Ok(_res) => {},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Received block_proposer_payload failed validation: {:?}", e);
					},
				};
			},
			NetworkEventType::ReceiveVote(vote_payload) => {
				let block_number = vote_payload.data.block_number;
				info!("🔍 🗳️ 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Vote from the network from: {} for Block #{}", hex::encode(&vote_payload.validator_address), block_number);
				
				match consensus.receive_vote(vote_payload, &db_pool_conn).await {
					Ok(_res) => {},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Received vote_payload failed validation, block #{}: {:?}", block_number, e);
					},
				};
			},
			NetworkEventType::ReceiveVoteResult(vote_result_payload) => {
				let block_number = vote_result_payload.data.block_number;
				info!("🔍 🗳️ 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Vote Result for Block #{}", block_number);
				match consensus.receive_vote_result(vote_result_payload, &db_pool_conn).await {
					Ok(_res) => {},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Received vote_result_payload failed validation, block #{}: {:?}", block_number, e);
					},
				};
			},
			NetworkEventType::ReceiveQueryBlockResponse( block_payload, vote_result ) => {
				let block_number = block_payload.block.block_header.block_number;
				// Validate and store the block
				info!("🔍 ℹ️ 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Block Response for Block #{}", block_number);
				match consensus.receive_block(block_payload, vote_result, &db_pool_conn).await{
					Ok(_res) => {
						log::info!("🔍 Node - Receive Network | Handle Network Event Type | Block Response successfully stored");
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Received block query payload failed validation: {:?}", e);
					},
				};
			},
			NetworkEventType::ReceiveQueryBlockRequest(request, channel) => {
				// Validate and store the block
				info!("🔍 ℹ️ 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Block Query Request for Block #{}", &request.block_number);
				let response = consensus.handle_query_block_request(request, &db_pool_conn).await.unwrap_or_else(|e| L1xResponse::QueryBlockError(e.to_string()));
				
				if let Err(e) =
					consensus.network_client_tx.send(BroadcastNetwork::BroadcastQueryBlockResponse (response, channel )).await
				{
					warn!("🚨 Node - Receive Network | Handle Network Event Type | Unable to write block request result to network_client_tx channel: {:?}", e)
				}			
			},
			NetworkEventType::ReceiveNodeHealth(node_healths) => {
				info!("🔍 💓 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Node Health from the network | Count: {}", node_healths.len());
                match consensus.receive_node_health(node_healths, db_pool_conn).await{
					Ok(_res) => {
						log::info!("🔍 💓 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Node Health from the network");
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Failed to select node health: {:?}", e);
					},
				};
            },
            NetworkEventType::ReceiveAggregatedNodeHealth(aggregated_healths) => {
				match consensus.receive_signed_node_health(aggregated_healths, db_pool_conn).await{
					Ok(_res) => {
						log::info!("🔍 💓 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Aggregated Node Health from the network");
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Failed to aggregate and broadcast node health: {:?}", e);
					},
				};
            },
			NetworkEventType::ProcessNodeHealth(epoch) => {
				match consensus.process_health_update(epoch, db_pool_conn).await{
					Ok(_res) => {
						log::info!("🔍 💓 🔗 Node - Receive Network | Handle Network Event Type | Computed and broadcast local node health");
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Failed to compute and broadcast node health: {:?}", e);
					},
				};
			},
			NetworkEventType::AggregateNodeHealth(epoch) => {
				match consensus.aggregate_and_broadcast_node_health(db_pool_conn, epoch).await{
					Ok(_res) => {
						log::info!("🔍 💓 🔗 Node - Receive Network | Handle Network Event Type | Aggregate and broadcast local node health , epoch: {:?}", epoch);
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Failed to aggregate and broadcast node health: {:?}", e);
					},
				};
			},
			NetworkEventType::PingResult(peer_id, is_success, rtt) => {
				consensus.handle_ping_result(peer_id, is_success, rtt).await;
			},
			NetworkEventType::PingEligiblePeers(epoch, peer_ids) => {
				consensus.handle_ping_eligible_peers(epoch, peer_ids).await;
			},
			NetworkEventType::CheckNodeStatus => {
				match consensus.request_node_status().await {
					Ok(_res) => {
						log::info!("🔍 ℹ️ 🔗 Node - Receive Network | Handle Network Event Type | I just requested a new Check Node Status");
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Failed to request node status: {:?}", e);
					},
				};
			},
			NetworkEventType::PublishNodeDetailedStatus(peer_id) => {
				match consensus.handle_publish_node_detailed_status(peer_id).await {
					Ok(_res) => {
						log::info!("🔍 ℹ️ 🔗 Node - Receive Network | Handle Network Event Type | I just published a new Node Detailed Status");
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Failed to publish node detailed status: {:?}", e);
					},
				};
			},
			NetworkEventType::ReceiveNodeDetailedStatus(node_detailed_status) => {
				match consensus.handle_receive_node_detailed_status(node_detailed_status.clone()).await {
					Ok(_res) => {
						log::info!("🔍 ℹ️ 🔗 Node - Receive Network | Handle Network Event Type | I just received a new Node Detailed Status");
					},
					Err(e) => {
						warn!("🚨 Node - Receive Network | Handle Network Event Type | Failed to receive node detailed status: {:?}", e);
					},
				};
			},
		}
	}
}

async fn set_initial_block_proposer<'a>(
	cluster_address: Address,
	node_address: Address,
	db_pool_conn: &'a DbTxConn<'a>,
	current_epoch: u64,
) {
	let block_proposer_state = BlockProposerState::new(db_pool_conn).await.expect("🚨 Node - Set Initial Block Proposer | failed to get block_proposer_state");
	let mut cluster_block_proposers = HashMap::new();
	cluster_block_proposers.insert(
		cluster_address.clone(),
		HashMap::from([(current_epoch, node_address.clone())]),
	);
	block_proposer_state.store_block_proposers(&cluster_block_proposers, Some(node_address.clone()), Some(cluster_address.clone()))
		.await.expect("🚨 Node - Set Initial Block Proposer | failed to set initial block proposer");
}

async fn broadcast_node_info<'a>(
	node_address: &Address,
	db_pool_conn: &'a DbTxConn<'a>,
	network_client_tx: mpsc::Sender<BroadcastNetwork>,
) {
	let node_info_state =
		NodeInfoState::new(&db_pool_conn).await.expect("🚨 Node - Broadcast Node Info | Unable to get node info state");

	let node_info = match node_info_state.load_node_info(node_address).await {
		Ok(info) => info,
		Err(e) =>
			panic!("🚨 Node - Broadcast Node Info | Failed to load node info because {}", e),
	};

	network_client_tx
		.send(BroadcastNetwork::BroadcastNodeInfo(node_info.clone()))
		.await
		.expect("🚨 Node - Broadcast Node Info | Unable to send node_info to network_client_tx");
}

pub async fn mempool_add_transaction(mempool_tx: mpsc::Sender<ProcessMempool>, transaction: Transaction) -> Result<(), NodeError> {
	// Submit transaction to the mempool
	let (response_mempool_tx, response_mempool_rx) = tokio::sync::oneshot::channel();
	match mempool_tx.send(ProcessMempool::AddTransaction(transaction.clone(), response_mempool_tx)).await {
		Ok(_) => (),
		Err(e) => {
			log::warn!("🚨 Node - Mempool Add Transaction | Unable to write transaction to mempool channel: {:?}", e);
			return Err(NodeError::UnexpectedError(format!(
				"🚨 Node - Mempool Add Transaction | Something went wrong: {:?}",
				e
			)));
		}
	}
	match response_mempool_rx.await {
		Ok(res) => match res {
			Ok(mempool_ack) => {
				match mempool_ack {
					ResponseMempool::Success => Ok(()),
					ResponseMempool::FailedToAddTransaction => {
						error!("🚨 Node - Mempool Add Transaction | Transaction has been dropped or not included in a block");
						Err(NodeError::UnexpectedError(format!("🚨 Node - Mempool Add Transaction | Transaction has been dropped or not included in a block")))
					}
				}
			}
			Err(e) => {
				error!("🚨 Node - Mempool Add Transaction | Received error in mempool receiver: {:?}", e);
				Err(NodeError::UnexpectedError(format!("🚨 Node - Mempool Add Transaction | Received error in mempool receiver: {:?}", e)))
			}
		},
		Err(e) => {
			error!("🚨 Node - Mempool Add Transaction | Failed to receive message from mempool receiver: {:?}", e);
			Err(NodeError::UnexpectedError(format!("🚨 Node - Mempool Add Transaction | Failed to receive message from mempool receiver: {:?}", e)))
		}
	}

}

pub async fn update_node_info(
	bootnodes: &[&str],
) -> Result<(), Error> {
	let grpc_clients = grpc_connect_bootnodes(bootnodes).await?;

	let db_pool_conn = Database::get_pool_connection().await?;
	let node_info_state = NodeInfoState::new(&db_pool_conn).await?;
	// Update existing node info
	for client_mutex in &grpc_clients {
		let mut client_guard = client_mutex.lock().await;
		let (grpc_client, endpoint) = &mut *client_guard;

		let request = l1x_rpc::rpc_model::GetNodeInfoRequest {};

		match grpc_client.get_node_info(request).await {
			Ok(response) => {
				for rpc_node_info in response.into_inner().node_info {
					let system_node_info = convert_to_system_node_info(rpc_node_info)?;
					node_info_state.store_node_info(&system_node_info).await?;
					info!("🔍 ℹ️ 🔗 Node - Update Node Info | Updated/Added node info for {}", hex::encode(&system_node_info.address));
				}
			},
			Err(e) => warn!("🚨 Node - Update Node Info | Failed to get all node info from {}: {}", endpoint, e),
		}
	}

	Ok(())
}

pub async fn update_genesis_block(
    bootnodes: &[&str],
) -> Result<(), Error> {
	let grpc_clients = grpc_connect_bootnodes(bootnodes).await?;

    let db_pool_conn = Database::get_pool_connection().await?;
    let block_state = BlockState::new(&db_pool_conn).await?;

    // Fetch and update genesis block
    for client_mutex in &grpc_clients {
        let mut client_guard = client_mutex.lock().await;
        let (grpc_client, endpoint) = &mut *client_guard;

        let request = GetGenesisBlockRequest {};

        match grpc_client.get_genesis_block(request).await {
            Ok(response) => {
                let genesis_block = response.into_inner().genesis_block
                    .ok_or_else(|| anyhow::anyhow!("🚨 Node - Update Genesis Block | No genesis block in response"))?;

                block_state.store_genesis_block_time(
                    genesis_block.block_number as u128,
                    genesis_block.epoch as u64
                ).await?;

                info!("🔍 🌱 🔗 Node - Update Genesis Block | Updated Genesis Block | Block Number: {}, Epoch: {}",
                      genesis_block.block_number, genesis_block.epoch);
                return Ok(());
            },
            Err(e) => warn!("🚨 Node - Update Genesis Block | Failed to get genesis block from {}: {}", endpoint, e),
        }
    }

    Err(anyhow::anyhow!("🚨 Node - Update Genesis Block | Failed to update genesis block from any bootnode"))
}

// Add constants for configurability
const HEALTH_SYNC_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RETRIES: u32 = 2;
const FALLBACK_EPOCH_LIMIT: u64 = 3; // Max epochs to look back

pub async fn update_node_healths(
	bootnodes: &[&str],
	epoch: Epoch,
) -> Result<(), Error> {
	let grpc_clients = grpc_connect_bootnodes(bootnodes).await?;
	let db_pool_conn = Database::get_pool_connection().await?;
	let node_health_state = Arc::new(NodeHealthState::new(&db_pool_conn).await?);

	for client_mutex in &grpc_clients {
		let mut client_guard = client_mutex.lock().await;
		let (grpc_client, endpoint) = &mut *client_guard;

		let endpoint_clone = endpoint.clone();
		let nhs_clone = Arc::clone(&node_health_state); // Clone the state handler

		// Helper for processing health entries
		let process_healths = |healths: Vec<l1x_rpc::rpc_model::NodeHealth>| async move {
			let mut last_error = None;
			let mut processed = 0;

			let health_count = healths.len();
			
			for rpc_node_health in healths {
				match convert_to_system_node_health(rpc_node_health) {
					Ok(system_node_health) => {
						if let Err(e) = nhs_clone.upsert_node_health(&system_node_health).await {
							last_error = Some(e.to_string());
							warn!("🚨 Node - Update Node Health | Partial failure updating {}: {}", system_node_health.measured_peer_id, e);
						} else {
							processed += 1;
						}
					},
					Err(e) => warn!("🚨 Node - Update Node Health | Invalid health data from {}: {}", endpoint.clone(), e),
				}
			}
			
			info!("🔍 ℹ️ 🔗 Node - Update Node Health | Processed {}/{} entries from {}", processed, health_count, endpoint);
			last_error
				.map(|e| Err(anyhow::anyhow!(e)))  // Convert String to anyhow::Error
				.unwrap_or(Ok(()))
		};

		// Retry with jittered backoff
		let mut attempt = 0;
		let mut response: Result<Option<Vec<l1x_rpc::rpc_model::NodeHealth>>, Error> = Ok(None);
		while attempt <= MAX_RETRIES {
			let delay = if attempt > 0 {
				let jitter = rand::random::<u64>() % 500;
				let delay_ms = (2u64.pow(attempt - 1) * 1000) + jitter;
				tokio::time::sleep(Duration::from_millis(delay_ms)).await;
			} else {
				futures::future::ready(()).await
			};

			match tokio::time::timeout(
				HEALTH_SYNC_TIMEOUT,
				grpc_client.get_node_healths(l1x_rpc::rpc_model::GetNodeHealthsRequest { epoch })
			).await {
				Ok(Ok(res)) => {
					response = Ok(Some(res.into_inner().node_healths));
					break;
				},
				Ok(Err(e)) => warn!("🚨 Node - Update Node Health | Attempt {} failed: {}", attempt, e),
				Err(_) => warn!("🚨 Node - Update Node Health | Timeout on attempt {}", attempt),
			}
			
			attempt += 1;
		}

		// Handle successful response
		if let Ok(Some(healths)) = response {
			process_healths(healths).await?;
			continue;
		}

		// Fallback to previous epochs with freshness check
		if epoch > 0 && (epoch.saturating_sub(FALLBACK_EPOCH_LIMIT)) < epoch {
			let fallback_epoch = epoch - 1;
			warn!("🚨 Node - Update Node Health | Falling back to epoch {} for {}", fallback_epoch, endpoint_clone);
			
			if let Ok(prev_response) = grpc_client.get_node_healths(
				l1x_rpc::rpc_model::GetNodeHealthsRequest { epoch: fallback_epoch }
			).await {
				process_healths(prev_response.into_inner().node_healths).await?;
			}
		}
	}
	
	Ok(())
}

async fn update_block_proposer_and_validator(
    bootnodes: &[&str], 
    epoch: Epoch, 
    cluster_address: Address
) -> Result<(), Error> {
    // Connect to bootnodes
    let grpc_clients = grpc_connect_bootnodes(bootnodes).await?;
    let db_pool_conn = Database::get_pool_connection().await?;
    
    // Collect responses from all bootnodes
    let mut proposer_responses = Vec::new();
    let mut validator_responses = Vec::new();
    let mut successful_connections = 0;
    
    // For each bootnode
    for client_mutex in &grpc_clients {
        let mut client_guard = client_mutex.lock().await;
        let (grpc_client, endpoint) = &mut *client_guard;
        
        // 1. Try to get block proposer with simple retry
        if let Some(response) = try_get_block_proposer(grpc_client, endpoint, epoch).await {
            proposer_responses.push(response);
            successful_connections += 1;
        }
        
        // 2. Try to get validators with simple retry
        if let Some(response) = try_get_validators(grpc_client, endpoint, epoch).await {
            validator_responses.push(response);
        }
    }

    // Make sure we have enough connections for quorum
    let quorum_threshold = match successful_connections {
        1 => 1,  // Single bootnode - trust it
        2 => 1,  // Two bootnodes - trust if at least one agrees
        _ => (successful_connections / 2) + 1  // Three or more - use majority
    };

    // Only fail if we have no successful connections
    if successful_connections == 0 {
        return Err(anyhow!("No successful connections to any bootnode"));
    }

    // 3. Process block proposers - find the most common one
    let most_common_proposer = find_most_common_proposer(&proposer_responses, quorum_threshold)?;
    
    // 4. Update block proposer if we have a quorum
    if let Some(proposer_address) = most_common_proposer {
        let block_proposer_state = BlockProposerState::new(&db_pool_conn).await?;
        block_proposer_state.upsert_block_proposer(
            cluster_address,
            epoch,
            proposer_address
        ).await?;
        info!("Block proposer updated with quorum: {}", hex::encode(proposer_address));
    } else {
        warn!("No quorum reached for block proposer selection");
    }

    // 5. Process validators - find the ones that appear in majority of responses
    let validators_with_quorum = find_validators_with_quorum(&validator_responses, quorum_threshold)?;
    
    // 6. Update validators if we have any with quorum
    if !validators_with_quorum.is_empty() {
        let validator_state = ValidatorState::new(&db_pool_conn).await?;
        validator_state.batch_store_validators(&validators_with_quorum).await?;
        info!("Validators updated with quorum: {} validators", validators_with_quorum.len());
    } else {
        warn!("No quorum reached for validator selection");
    }

    Ok(())
}

// Helper function to get block proposer with retry
async fn try_get_block_proposer(
    grpc_client: &mut NodeClient<tonic::transport::Channel>,
    endpoint: &str,
    epoch: Epoch
) -> Option<l1x_rpc::rpc_model::GetBpForEpochResponse> {
    let request = l1x_rpc::rpc_model::GetBpForEpochRequest { epoch };
    
    for attempt in 0..3 {
        match grpc_client.get_block_proposer_for_epoch(request.clone()).await {
            Ok(response) => {
				debug!("🔍 ℹ️ 🔗 Node - Try Get Block Proposer | Got block proposer from {} for epoch {}", endpoint, epoch);
				return Some(response.into_inner());
            },
            Err(e) => {
                if attempt < 2 {
                    // Simple backoff: wait longer for each retry
                    let delay = (attempt + 1) * 1000;
                    tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                    warn!("🔍 ℹ️ 🔗 Node - Try Get Block Proposer | Failed to get block proposer from {} for epoch {} (retry {}/3): {}", endpoint, epoch, attempt + 1, e);
                } else {
                    warn!("🔍 ℹ️ 🔗 Node - Try Get Block Proposer | Failed to get block proposer from {} for epoch {} after 3 attempts: {}", endpoint, epoch, e);
                }
            }
        }
    }
    
    None
}

// Helper function to get validators with retry
async fn try_get_validators(
    grpc_client: &mut NodeClient<tonic::transport::Channel>,
    endpoint: &str,
    epoch: Epoch
) -> Option<l1x_rpc::rpc_model::GetValidatorsForEpochResponse> {
    let request = l1x_rpc::rpc_model::GetValidatorsForEpochRequest { epoch };
    
    for attempt in 0..3 {
        match grpc_client.get_validators_for_epoch(request.clone()).await {
            Ok(response) => return Some(response.into_inner()),
            Err(e) => {
                if attempt < 2 {
                    // Simple backoff: wait longer for each retry
                    let delay = (attempt + 1) * 1000;
                    tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                    warn!("Failed to get validators from {} (retry {}/3): {}", endpoint, attempt + 1, e);
                } else {
                    warn!("Failed to get validators from {} after 3 attempts: {}", endpoint, e);
                }
            }
        }
    }
    
    None
}

// Find the most common proposer that meets quorum threshold
fn find_most_common_proposer(
    responses: &[l1x_rpc::rpc_model::GetBpForEpochResponse],
    quorum_threshold: usize
) -> Result<Option<Address>, Error> {
    let mut proposer_counts = HashMap::new();
    
    // Count occurrences of each proposer
    for response in responses {
        for bp in &response.bp_for_epoch {
            *proposer_counts.entry(bp.bp_address.clone()).or_insert(0) += 1;
        }
    }
    
    // Find the most common proposer that meets quorum
    let most_common = proposer_counts.into_iter()
        .filter(|&(_, count)| count >= quorum_threshold)
        .max_by_key(|&(_, count)| count);
    
    if let Some((address_str, _)) = most_common {
        let address = Address::try_from(address_str)
            .map_err(|_| anyhow::anyhow!("🚨 Node - Find Most Common Proposer | Invalid address"))?;
        Ok(Some(address))
    } else {
        Ok(None)
    }
}

// Find validators that appear in enough responses to meet quorum
fn find_validators_with_quorum(
    responses: &[l1x_rpc::rpc_model::GetValidatorsForEpochResponse],
    quorum_threshold: usize
) -> Result<Vec<Validator>, Error> {
    let mut validator_counts = HashMap::new();
    let mut validator_details = HashMap::new();
    
    // Count each validator and store details
    for response in responses {
        for validators_for_epoch in &response.validators_for_epochs {
            for validator in &validators_for_epoch.validators {
                *validator_counts.entry(validator.address.clone()).or_insert(0) += 1;
                validator_details.insert(validator.address.clone(), validator.clone());
            }
        }
    }
    
    // Collect validators that meet quorum
    let mut validators_with_quorum = Vec::new();
    for (address_str, count) in validator_counts {
        if count >= quorum_threshold {
            if let Some(detail) = validator_details.get(&address_str) {
                let validator = Validator {
                    address: Address::try_from(address_str)
                        .map_err(|_| anyhow::anyhow!("🚨 Node - Find Validators With Quorum | Invalid address"))?,
                    cluster_address: Address::try_from(detail.cluster_address.clone())
                        .map_err(|_| anyhow::anyhow!("🚨 Node - Find Validators With Quorum | Invalid cluster address"))?,
                    epoch: detail.epoch,
                    stake: detail.stake.parse::<u128>()
                        .map_err(|_| anyhow::anyhow!("🚨 Node - Find Validators With Quorum | Invalid stake"))?,
                    xscore: detail.xscore,
                };
                validators_with_quorum.push(validator);
            }
        }
    }
    
    Ok(validators_with_quorum)
}

fn convert_to_system_node_info(node_info: l1x_rpc::rpc_model::NodeInfo) -> Result<NodeInfo, Error> {
	Ok(NodeInfo {
		address: Address::try_from(node_info.address)
			.map_err(|_| anyhow::anyhow!("🚨 Node - Convert to System Node Info | Invalid address"))?,
		peer_id: node_info.peer_id,
		data: NodeInfoSignPayload {
			ip_address: node_info.ip_address,
			metadata: node_info.metadata,
			cluster_address: Address::try_from(node_info.cluster_address)
				.map_err(|_| anyhow::anyhow!("🚨 Node - Convert to System Node Info | Invalid cluster address"))?,
		},
		joined_epoch: node_info.joined_epoch,
		signature: node_info.signature,
		verifying_key: node_info.verifying_key,
	})
}

fn convert_to_system_node_health(node_health: l1x_rpc::rpc_model::NodeHealth) -> Result<NodeHealth, Error> {
	Ok(NodeHealth {
		measured_peer_id: node_health.measured_peer_id,
		peer_id: node_health.peer_id,
		epoch: node_health.epoch,
		joined_epoch: node_health.joined_epoch,
		uptime_percentage: node_health.uptime_percentage,
		response_time_ms: node_health.response_time_ms,
		transaction_count: node_health.transaction_count,
		block_proposal_count: node_health.block_proposal_count,
		anomaly_score: node_health.anomaly_score,
		node_health_version: node_health.node_health_version,
	})
}

async fn try_initialize_runtime_configs(cluster_address: Address) -> Result<(), Error> {
	match tokio::task::spawn_blocking(move || {
		let result =  async_scoped::TokioScope::scope_and_block(|scope|{
			scope.spawn_blocking(|| {
				let res: Result<(), Error> = tokio::runtime::Runtime::new().expect("🚨 Node - Try Initialize Runtime Configs | Can't create tokio runtime").block_on(async {
						let (tx, _drop_guard) = state::updated_state_db::run_db_handler().await?; 
						let updated_state = state::UpdatedState::new(tx);
						let db_pool_conn = db::db::Database::get_pool_connection().await?;
						let block_state = BlockState::new(&db_pool_conn).await?;
						let block_header = block_state.block_head_header(cluster_address).await?;
						let _ = execute::execute_block::refresh_system_runtime_config(&updated_state, &block_header).await;
						let _ = execute::execute_block::refresh_system_staking_config(&updated_state, &block_header).await;
						let _ = execute::execute_block::refresh_system_runtime_deny_config(&updated_state, &block_header).await;
	
						Ok(())
					});
				res
			})
		});

		if let Some(res) = result.1.into_iter().next() {
			match res {
				Ok(res) => {
					res
				},
				Err(e) => {
					error!("🚨 Node - Try Initialize Runtime Configs | Join Error: {e}");
					Err(e)?
				},
			}
		} else {
			error!("🚨 Node - Try Initialize Runtime Configs | No results after scope_and_block");
			Err(anyhow::anyhow!("🚨 Node - Try Initialize Runtime Configs | No results after scope_and_block"))
		}
	}).await {
		Ok(res) => res,
		Err(e) => Err(anyhow::anyhow!("🚨 Node - Try Initialize Runtime Configs | Join Error: {e}")),
	}
}

pub async fn grpc_connect_bootnodes(
	bootnodes: &[&str],
) -> Result<Vec<Arc<Mutex<(NodeClient<tonic::transport::Channel>, String)>>>, Error> {

	info!("🔍  📡 🔗 Node - Grpc Connect Bootnodes | bootnodes: {:?}", bootnodes);

	let sync_endpoints = multiaddrs_to_http_urls(bootnodes);
	let mut grpc_clients = Vec::new();

	for endpoint in sync_endpoints {
		match NodeClient::connect(endpoint.clone()).await {
			Ok(grpc_client) => {
				info!("🔍  📡 🔗 Node - Grpc Connect Bootnodes | Connection successful to endpoint: {}", endpoint);
				// Increase max message size because Block can be bigger than 4Mb (default value)
				let grpc_client = grpc_client.max_decoding_message_size(usize::MAX);
				grpc_clients.push(Arc::new(Mutex::new((grpc_client, endpoint))));
			},
			Err(err) => warn!("🚨 Node - Grpc Connect Bootnodes | Failed to connect to endpoint {}: {:?}", endpoint, err),
		}
	}

	if grpc_clients.is_empty() {
		return Err(anyhow::anyhow!("🚨 Node - Grpc Connect Bootnodes | No boot nodes available"));
	}

	Ok(grpc_clients)
}