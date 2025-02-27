use db::db::Database;
use libp2p::{autonat, futures::StreamExt , futures::stream::FuturesUnordered, swarm::ConnectionHandlerUpgrErr};
use libp2p_gossipsub::{self as gossipsub, MessageId};
use primitives::{Address, Epoch};
use void::Void;
use crate::config::*;
use libp2p::{
	core::upgrade,
	identify,
	kad::{record::store::MemoryStore, Kademlia, KademliaConfig, KademliaEvent, QueryResult},
};
use libp2p::{
	identity,
	identity::Keypair,
	noise,
	swarm::{NetworkBehaviour, Swarm, SwarmBuilder, SwarmEvent},
	tcp, yamux, Multiaddr, PeerId, Transport,
	request_response::{self, *},
};
use libp2p::multiaddr::Protocol;
use std::{collections::{hash_map::{self, DefaultHasher}, HashMap}, str::FromStr};
use futures::{AsyncRead, AsyncWrite, AsyncWriteExt, AsyncReadExt};
use std::io;
use itertools::Itertools;
use std::{
	error::Error,
	hash::{Hash, Hasher},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, error, info, warn};

use std::time::Duration;
use system::{
	block::{BlockBroadcast, BlockPayload, BlockQueryRequest, L1xRequest, L1xResponse, QueryBlockMessage, QueryBlockResponse, QueryStatusRequest},
	block_proposer::{BlockProposerBroadcast, BlockProposerPayload}, dht_health_storage::DHTHealthStorage,
	node_health::{AggregatedNodeHealthBroadcast, NodeHealthBroadcast, NodeHealthPayload},
	node_info::{NodeInfo, NodeInfoBroadcast}, protocol::{deserialize_from_versioned_message, serialize_as_versioned_message},
	transaction::{Transaction, TransactionBroadcast}, vote::{Vote, VoteBroadcast}, vote_result::{VoteResult, VoteResultBroadcast}
};
use tokio::sync::{mpsc, oneshot};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use block::block_state::BlockState;
use block::block_manager::BlockManager;
use system::node_health::NodeHealth;
use compile_time_config::{ NODE_STATUS_TIMER, ELIGIBLE_PEERS_INIT_BLOCK_NUMBER };
use lazy_static::lazy_static;
use std::sync::Mutex;

fn deserialize<'a, R: Deserialize<'a>>(encoded_data: &'a [u8]) -> Result<R, io::Error> {
	bincode::deserialize(encoded_data).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

fn serialize<D: Serialize>(data: &D) -> Result<Vec<u8>, io::Error> {
	bincode::serialize(data).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

const MAX_CLOSEST_PEERS: usize = 10;

// Implement Lazy Static for ACTIVE_PEERS
lazy_static! {
	static ref ACTIVE_PEERS: Mutex<Vec<PeerId>> = Mutex::new(Vec::new());
}

/// Creates the network components, namely:
///
/// - The network client to interact with the network layer from anywhere within your application.
///
/// - The network event stream, e.g. for incoming requests.
///
/// - The network task driving the network itself.
pub async fn new(
	local_keys: Keypair,
	bootnodes: &[&str],
	dht_health_storage: DHTHealthStorage,
	autonat_config: &Option<system::config::AutonatConfig>,
) -> Result<(Client, mpsc::Receiver<Event>, EventLoop), Box<dyn Error>> {
	// Create a public/private key pair, either random or based on a seed.
	let local_peer_id = local_keys.public().to_peer_id();

	let _tcp_transport = libp2p::tokio_development_transport(local_keys.clone())?;

	let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
		.upgrade(upgrade::Version::V1)
		.authenticate(noise::Config::new(&local_keys).expect("signing libp2p-noise static keypair"))
		.multiplex(yamux::Config::default())
		.timeout(std::time::Duration::from_secs(20))
		.boxed();

	// Create new topic for transactions
	let node_join_topic = get_topic_hash(NODE_INFO_TOPIC);
	let tx_topic = get_topic_hash(TRANSACTIONS_TOPIC);
	let block_validate_topic = get_topic_hash(BLOCKS_VALIDATE_TOPIC);
	let block_proposer_topic = get_topic_hash(BLOCK_PROPOSER_TOPIC);
	let vote_topic = get_topic_hash(VOTE_TOPIC);
	let vote_result_topic = get_topic_hash(VOTE_RESULT_TOPIC);
	let node_health_topic = get_topic_hash(NODE_HEALTH_TOPIC);
	let aggregated_node_health_topic = get_topic_hash(AGGREGATED_NODE_HEALTH_TOPIC);

	let topics = vec![
		node_join_topic,
		tx_topic,
		vote_result_topic,
		block_validate_topic,
		block_proposer_topic,
		vote_topic,
		node_health_topic,
		aggregated_node_health_topic,
	];

	// Only the very first node should start subscribed to these validator only topics
	// as it is hardcoded to be a validator from genesis
	// if bootnodes.is_empty() {
	// 	topics.push(block_validate_topic);
	// 	topics.push(block_proposer_topic);
	// 	topics.push(vote_topic);
	// }

	// Build the Swarm, connecting the lower layer transport logic with the
	// higher layer network behaviour logic.
	let swarm = {
		let mut behaviour = Behaviour {
			identify: identify::Behaviour::new(identify::Config::new(
				"/ipfs/id/1.0.0".to_string(),
				local_keys.public(),
			)),
			// mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?,
			kademlia: kademlia_behaviour(local_peer_id),
			gossipsub: gossipsub_behaviour(local_keys.clone(), topics)?,
			auto_nat: autonat_behaviour(local_peer_id, autonat_config),
			request_response: libp2p::request_response::Behaviour::new(
				L1xCodec,
				vec![(L1xProtocol, ProtocolSupport::Full)],
				request_response::Config::default(),
			),
		};

		// If provided, bootstrap routing table with bootnode(s)
		if !bootnodes.is_empty() {
			for bootnode in bootnodes {
				info!("Bootstrapping to: {}", bootnode);
				let (peer, _, address) = bootnode.rsplitn(3, "/").into_iter().tuples().next().ok_or(anyhow!("Invalid bootnode address, expecting format /ip4/<address>/tcp/<port>/p2p/<peer_id>, got {bootnode}"))?;
				behaviour.kademlia.add_address(&peer.parse()?, address.parse()?);
				behaviour.auto_nat.add_server(
					PeerId::from_str(peer).unwrap(),
					Some(Multiaddr::from_str(bootnode).unwrap()),
				);
			}

			behaviour.kademlia.bootstrap()?;
		}

		SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build()
	};

	let (command_sender, command_receiver) = mpsc::channel(1000);
	let (event_sender, event_receiver) = mpsc::channel(1000);
	let client = Client { sender: command_sender, event_sender: event_sender.clone() };
	let event_loop = EventLoop::new(swarm, command_receiver, event_sender, dht_health_storage);
	
	// Start peer discovery
	// match client.start_peer_discovery().await {
	// 	Ok(_) => {},
	// 	Err(e) => {
	// 		error!("Failed to start peer discovery: {:?}", e);
	// 	}
	// };

	Ok((
		client,
		event_receiver,
		event_loop,
	))
}

#[derive(Clone)]
pub struct Client {
	sender: mpsc::Sender<Command>,
	event_sender: mpsc::Sender<Event>,
}

impl Client {
	/// Listen for incoming connections on the given address.
	pub async fn start_listening(&mut self, addr: Multiaddr) -> Result<(), Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::StartListening { addr, sender })
			.await
			.expect("Command receiver not to be dropped.");
		receiver.await.expect("Sender not to be dropped.")
	}

	/// Command the node to subscribe to gossipsub messages published under the given `topic`
	pub async fn subscribe_to_topic(&mut self, topic: String) -> Result<(), Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::TopicSubscribe { topic_string: topic, sender })
			.await
			.expect("Command receiver not to be dropped.");
		receiver.await.expect("Sender not to be dropped.")
	}

	/// Command the node to unsubscribe from the gossipsub messages published under the given
	/// `topic`
	pub async fn unsubscribe_from_topic(
		&mut self,
		topic: String,
	) -> Result<(), Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::TopicUnsubscribe { topic_string: topic, sender })
			.await
			.expect("Command receiver not to be dropped.");
		receiver.await.expect("Sender not to be dropped.")
	}

	pub async fn start_node_health_monitoring(
		&self,
		cluster_address: Address,
	) -> Result<(), Box<dyn Error + Send>> {
		let sender = self.event_sender.clone();
		log::info!("🏥 Starting node health monitoring");
		let block_manager = BlockManager{};

		tokio::spawn(async move {
			let db_pool_conn = Database::get_pool_connection().await.unwrap();
			let block_state = BlockState::new(&db_pool_conn).await.unwrap();
			let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3));
			let mut last_processed_epoch: Option<Epoch> = None;
			let mut last_aggregated_epoch: Option<Epoch> = None;

			loop {
				interval.tick().await;

				let header = match block_state.load_chain_state(cluster_address.clone()).await {
					Ok(h) => h,
					Err(e) => {
						log::error!("Unable to get latest block head: {}", e);
						continue;
					}
				};

				let current_epoch = match block_manager.calculate_current_epoch(header.block_number) {
					Ok(epoch) => epoch,
					Err(e) => {
						log::error!("Unable to calculate current epoch: {}", e);
						continue;
					}
				};

				match block_state.is_block_executed(header.block_number, &cluster_address).await {
					Ok(true) => {
						log::debug!("start_node_health_monitoring ~ Block is executed, processing node health for epoch: {}, last_processed_epoch: {:?}, block_number: {}", current_epoch.clone(), last_processed_epoch.clone(), header.block_number.clone());

						// Process node health
						if last_processed_epoch != Some(current_epoch) {
							match block_manager.is_approaching_epoch_end(header.block_number, 5).await {
								Ok(true) => {
									if let Err(e) = sender.send(Event::ProcessNodeHealth{epoch: current_epoch}).await {
										log::error!("Failed to send ProcessNodeHealth event: {}", e);
									} else {
										last_processed_epoch = Some(current_epoch);
									}
								},
								Ok(false) => {},
								Err(e) => {
									log::error!("Error checking if epoch is approaching end: {}", e);
								}
							}
						}

						log::debug!("start_node_health_monitoring ~ Last aggregated epoch: {:?}", last_aggregated_epoch.clone());
						// Aggregate node health
						if last_aggregated_epoch != Some(current_epoch) {
							match block_manager.is_approaching_epoch_end(header.block_number, 10).await {
								Ok(true) => {
									if let Err(e) = sender.send(Event::AggregateNodeHealth {epoch: current_epoch}).await {
										log::error!("Failed to send health update command: {}", e);
									} else {
										last_aggregated_epoch = Some(current_epoch);
									}
								},
								Ok(false) => {},
								Err(e) => {
									log::error!("Error checking if epoch is approaching end: {}", e);
								}
							}
						}
					},
					Ok(false) => {
						log::debug!("start_node_health_monitoring ~ Block is not executed, skipping node health aggregation");
						continue
					},
					Err(e) => {
						log::warn!("Error checking if block is executed: {}", e);
					}
				};
			}
		});

		Ok(())
	}

	pub async fn start_node_monitoring(
		&self,
	) -> Result<(), Box<dyn Error + Send>> {
		info!("start_node_monitoring ~ Node monitoring has started");
		let sender = self.event_sender.clone();
		let timer_duration = Duration::from_secs(NODE_STATUS_TIMER);
		tokio::spawn(async move {
			loop {
				sleep(timer_duration).await;
				if let Err(e) = sender.send(Event::CheckNodeStatus).await {
					log::error!("Failed to send check-node-status command: {}", e);
				}
			}
		});
		Ok(())
	}

	pub async fn start_peer_discovery(&self) -> Result<(), Box<dyn Error + Send>> {
		let sender = self.event_sender.clone();
		
		tokio::spawn(async move {
			let mut interval = tokio::time::interval(Duration::from_secs(10)); // Adjust interval as needed
			
			loop {
				interval.tick().await;
				
				// Send event to initialize/refresh peers
				if let Err(e) = sender.send(Event::InitializePeers).await {
					error!("Failed to send InitializePeers event: {}", e);
				}
			}
		});
		
		Ok(())
	}

	pub async fn start_ping_eligible_peers(&self, current_epoch: Epoch) {
		let sender = self.event_sender.clone();
		let mut interval = tokio::time::interval(Duration::from_secs(10));
		tokio::spawn(async move {
			loop {
				interval.tick().await;

				let active_peers = match ACTIVE_PEERS.lock() {
					Ok(peers) => peers.iter().map(|peer| peer.to_string()).collect(),
					Err(e) => {
						error!("start_ping_eligible_peers ~ Failed to lock ACTIVE_PEERS: {:?}", e);
						return;
					}
				};

				info!("start_ping_eligible_peers ~ Active peers: {:?}", active_peers);

				let _ = sender
					.send(Event::PingEligiblePeers {
						epoch: current_epoch, // Or determine current epoch
						peer_ids: active_peers
					})
					.await
					.unwrap_or_else(|e| error!("Failed to send PingEligiblePeers event: {:?}", e));

				
			}
		});
	}
	
	
}

#[async_trait]
impl NodeInfoBroadcast for Client {
	/// Command the o broadcast a node_info to the p2p network.
	async fn node_info_broadcast(
		&self,
		node_info: NodeInfo,
	) -> Result<MessageId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastNodeInfo { node_info, sender })
			.await
			// .expect("Command receiver not to be dropped.");
			.unwrap_or_else(|e| {
				error!(
					"Failed to send BroadcastNodeInfo command down command_sender channel: {:?}",
					e
				)
			});
		match receiver.await {
			Ok(res) => match res {
				Ok(msg_id) => Ok(msg_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl TransactionBroadcast for Client {
	/// Command the node to broadcast a transaction to the p2p network.
	async fn transaction_broadcast(&self, transaction: Transaction) -> Result<MessageId> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastTransaction { transaction, sender })
			.await
			.unwrap_or_else(|e| {
				error!(
					"Failed to send BroadcastTransaction command down command_sender channel: {:?}",
					e
				)
			});
		match receiver.await {
			Ok(res) => match res {
				Ok(msg_id) => Ok(msg_id),
				Err(e) => Err(anyhow!("Failed to broadcast transaction: {:?}", e)),
			},
			Err(e) => Err(anyhow::Error::from(e)),
		}
	}
}

#[async_trait]
impl NodeHealthBroadcast for Client {
	/// Command the node to broadcast node health to the p2p network.
	async fn node_health_broadcast(&self, node_healths: Vec<NodeHealth>) -> Result<MessageId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastNodeHealth { node_healths, sender })
			.await
			.unwrap_or_else(|e| {
				error!(
					"Failed to send BroadcastTransaction command down command_sender channel: {:?}",
					e
				)
			});
		match receiver.await {
			Ok(res) => match res {
				Ok(msg_id) => Ok(msg_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl AggregatedNodeHealthBroadcast for Client {
	/// Command the node to broadcast node health to the p2p network.
	async fn aggregated_node_health_broadcast(&self, node_health_payloads: Vec<NodeHealthPayload>) -> Result<MessageId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastNodeHealthPayload { node_health_payloads, sender })
			.await
			.unwrap_or_else(|e| {
				error!(
					"Failed to send BroadcastTransaction command down command_sender channel: {:?}",
					e
				)
			});
		match receiver.await {
			Ok(res) => match res {
				Ok(msg_id) => Ok(msg_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl BlockBroadcast for Client {
	async fn block_validate_broadcast(
		&self,
		block_payload: BlockPayload,
	) -> Result<MessageId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastValidateBlock { block_payload, sender })
			.await
			.unwrap_or_else(|e| {
				error!("Failed to send BroadcastBlock command down command_sender channel: {e:?}");
			});

		match receiver.await {
			Ok(res) => match res {
				Ok(msg_id) => Ok(msg_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl BlockProposerBroadcast for Client {
	async fn block_proposer_broadcast(
		&self,
		block_proposer_payload: BlockProposerPayload,
	) -> Result<MessageId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastBlockProposer { block_proposer_payload, sender })
			.await
			.unwrap_or_else(|e| {
				error!("Failed to send BroadcastBlockProposer command down command_sender channel: {e:?}");
			});

		match receiver.await {
			Ok(res) => match res {
				Ok(msg_id) => Ok(msg_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl VoteBroadcast for Client {
	async fn vote_broadcast(&self, vote: Vote) -> Result<MessageId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastVote { vote, sender })
			.await
			.unwrap_or_else(|e| {
				error!("Failed to send BroadcastVote command down command_sender channel: {e:?}");
			});

		match receiver.await {
			Ok(res) => match res {
				Ok(msg_id) => Ok(msg_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl VoteResultBroadcast for Client {
	async fn vote_result_broadcast(
		&self,
		vote_result: VoteResult,
	) -> Result<MessageId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastVoteResult { vote_result, sender })
			.await
			.unwrap_or_else(|e| {
				error!(
					"Failed to send BroadcastVoteResult command down command_sender channel: {e:?}"
				);
			});

		match receiver.await {
			Ok(res) => match res {
				Ok(msg_id) => Ok(msg_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl BlockQueryRequest for Client {
	async fn block_query_request(
		&self,
		request: QueryBlockMessage,
		peer_id: PeerId,
	) -> Result<RequestId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::QueryBlockRequest { peer_id, request: L1xRequest::QueryBlock(request), sender })
			.await
			.unwrap_or_else(|e| {
				error!(
					"Failed to query for block command down command_sender channel: {e:?}"
				);
			});

		match receiver.await {
			Ok(res) => match res {
				Ok(request_id) => Ok(request_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl QueryBlockResponse for Client {
	async fn block_query_response(
		&self,
		response: L1xResponse,
		channel: ResponseChannel<L1xResponse>
	) -> Result<(), Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::QueryBlockResponse { channel, response, sender})
			.await
			.unwrap_or_else(|e| {
				error!(
					"Failed to send response for query block command down command_sender channel: {e:?}"
				);
			});

		match receiver.await {
			Ok(res) => match res {
				Ok(_) => Ok(()),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[async_trait]
impl QueryStatusRequest for Client {
	async fn query_status_request(
		&self,
		request_time: u128,
		peer_id: PeerId,
	) -> Result<RequestId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::QueryStatusRequest { peer_id, request: L1xRequest::QueryNodeStatus(request_time), sender })
			.await
			.unwrap_or_else(|e| {
				error!(
					"Failed to query for status command down command_sender channel: {e:?}"
				);
			});

		match receiver.await {
			Ok(res) => match res {
				Ok(request_id) => Ok(request_id),
				Err(e) => Err(e),
			},
			Err(e) => Err(Box::new(e)),
		}
	}
}

pub struct EventLoop {
	swarm: Swarm<Behaviour>,
	command_receiver: mpsc::Receiver<Command>,
	event_sender: mpsc::Sender<Event>,
	pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>,
}

impl EventLoop {
	fn new(
		swarm: Swarm<Behaviour>,
		command_receiver: mpsc::Receiver<Command>,
		event_sender: mpsc::Sender<Event>,
		dht_health_storage: DHTHealthStorage,
	) -> Self {
		Self { swarm, command_receiver, event_sender, pending_dial: Default::default() }
	}

	/// Start the event loop. This will listen for commands from the node (itself) and events from
	/// p2p network,
	pub async fn run(mut self) {
		loop {
			tokio::select! {
				event = self.swarm.select_next_some() => {
					let event = event;
					self.handle_event(event).await;
				},
				command = self.command_receiver.recv() => match command {
					Some(c) => self.handle_command(c).await,
					// Command channel closed, thus shutting down the network event loop.
					None => {
						warn!("event_loop worker exited");
						return
					},
				},
			}
		}
	}

	
	async fn add_active_peer(&mut self, peer_id: PeerId) -> Result<(), anyhow::Error> {
		let mut peers = match ACTIVE_PEERS.lock() {
			Ok(peers) => peers,
			Err(e) => {
				error!("Failed to lock ACTIVE_PEERS: {:?}", e);
				return Err(anyhow!("Failed to lock ACTIVE_PEERS"));
			}
		};

		if !peers.contains(&peer_id) {
			peers.push(peer_id);
		}

		Ok(())
	}

	async fn remove_active_peer(&mut self, peer_id: PeerId) -> Result<(), anyhow::Error> {
		let mut peers = match ACTIVE_PEERS.lock() {
			Ok(peers) => peers,
			Err(e) => {
				error!("Failed to lock ACTIVE_PEERS: {:?}", e);
				return Err(anyhow!("Failed to lock ACTIVE_PEERS"));
			}
		};

		let peer_to_remove = peers.iter().position(|id| id == &peer_id);
		if let Some(index) = peer_to_remove {
			peers.remove(index);
		}
	
		Ok(())
	}
	
	/// Handles events received from the p2p network. This can result in anything from logging some
	/// info to sending a transaction for mempool validation.
	async fn handle_event(
		&mut self,
		event: SwarmEvent<BehaviourEvent, either::Either<either::Either<either::Either<either::Either<std::io::Error, std::io::Error>, Void>, ConnectionHandlerUpgrErr<std::io::Error>>, ConnectionHandlerUpgrErr<std::io::Error>>>,
	) {

		match event {
			SwarmEvent::NewListenAddr { address, .. } => {
				let local_peer_id = *self.swarm.local_peer_id();
				info!(
					"Local node is listening on {:?}",
					address.with(Protocol::P2p(local_peer_id.into()))
				);
			},
			SwarmEvent::IncomingConnection { .. } => {},
			SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } =>
				if endpoint.is_dialer() {
					if let Some(sender) = self.pending_dial.remove(&peer_id) {
						let _ = sender.send(Ok(()));
					}

					debug!("ConnectionEstablished ~ Peer ID: {:?}", peer_id);

					// Update the active peers to lazy static ACTIVE_PEERS
					match self.add_active_peer(peer_id).await {
						Ok(_) => debug!("ConnectionEstablished ~ Peer ID: {:?} added to ACTIVE_PEERS", peer_id),
						Err(e) => error!("ConnectionEstablished ~ Peer ID: {:?} failed to add to ACTIVE_PEERS: {:?}", peer_id, e),
					}
					

				},
			SwarmEvent::ConnectionClosed { peer_id, cause, endpoint, .. } => {
				if let Some(cause) = cause {
					warn!("Connection closed with peer {}: {}", peer_id, cause);
				} else {
					warn!("Connection closed with peer {}", peer_id);
				}
				self.handle_peer_disconnection(peer_id).await;

				debug!("ConnectionClosed ~ Peer ID: {:?}", peer_id);
				
				match self.remove_active_peer(peer_id).await {
					Ok(_) => debug!("ConnectionClosed ~ Peer ID: {:?} removed from ACTIVE_PEERS", peer_id),
					Err(e) => error!("ConnectionClosed ~ Peer ID: {:?} failed to remove from ACTIVE_PEERS: {:?}", peer_id, e),
				}
			},
			SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
				if let Some(peer_id) = peer_id {
					if let Some(sender) = self.pending_dial.remove(&peer_id) {
						let _ = sender.send(Err(Box::new(error)));
					}
				}
			},
			SwarmEvent::IncomingConnectionError{error, local_addr, send_back_addr} => {
				warn!("Error: local_addr={local_addr:?}, send_back_addr={send_back_addr:?}, error={error:?}")
			},
			SwarmEvent::Dialing(peer_id) => info!("Dialing {peer_id}"),
			SwarmEvent::Behaviour(event) => match event {
				BehaviourEvent::Identify(event) => match event {
					
					// Prints peer id identify info is being sent to.
					identify::Event::Sent { peer_id, .. } => {
						info!("Sent identify info to {peer_id:?}")
					},
					// Prints out the info received via the identify event
					identify::Event::Received { peer_id, info } => {
						info!("Received {info:?} from {peer_id:?}");
						let local_peer_id = self.swarm.local_peer_id().clone();
						self.swarm.behaviour_mut().kademlia.get_closest_peers(local_peer_id);
					},
					_ => info!("Some Identify event received: {event:?}"),
				},
				BehaviourEvent::Kademlia(event) => match event {
					KademliaEvent::OutboundQueryProgressed { result, id, .. } => match result {
						QueryResult::Bootstrap(result) => match result {
							Ok(res) => {
								info!("BOOTSTRAP SUCCESS: {res:?}");
								// Initialize peers after successful bootstrap
								let mut peer_ids: Vec<String> = Vec::new();
								for bucket in self.swarm.behaviour_mut().kademlia.kbuckets() {
									for entry in bucket.iter() {
										let peer_id = entry.node.key.clone().into_preimage();
										peer_ids.push(peer_id.to_string());
									}
								}
								
								if !peer_ids.is_empty() {
									let _ = self.event_sender
										.send(Event::PingEligiblePeers {
											epoch: 0, // Or determine current epoch
											peer_ids
										})
										.await
										.unwrap_or_else(|e| error!("Failed to send PingEligiblePeers event: {:?}", e));
								}
							},
							Err(e) => {
								error!("BOOTSTRAP FAILURE: {e:?}");
							},
						},
						QueryResult::GetClosestPeers(result) => match result {
							Ok(res) => {
								let mut futures = FuturesUnordered::new();
								let closest_peers = res.peers.clone();

								for peer_id in res.peers.iter().take(MAX_CLOSEST_PEERS) { // Connect to up to 5 peers
									let addrs = self.swarm.behaviour_mut().kademlia.addresses_of_peer(peer_id)
										.into_iter()
										.filter(is_public_address)
										.collect::<Vec<_>>();
									for addr in addrs {
										let (oneshot_tx, oneshot_rx) = oneshot::channel();
										let dial_cmd = Command::Dial { peer_id: *peer_id, peer_addr: addr.clone(), sender: oneshot_tx };
										self.handle_command(dial_cmd).await;
										futures.push(async move { (*peer_id, oneshot_rx.await) });
									}
								}
								
								while let Some((peer_id, result)) = futures.next().await {
									match result {
										Ok(Ok(())) => info!("Successfully connected to a peer: {peer_id:}"),
										Ok(Err(e)) => {
											debug!("Failed to connect to a peer: {peer_id:}, error: {e:}");
											// Remove the peer from Kademlia and local store
											self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
										},
										Err(e) => debug!("Failed to receive result for peer: {peer_id:}, error: {e:}"),
									}
								}

								let mut peer_ids: Vec<String> = Vec::new();
								for peer_id in closest_peers {
									peer_ids.push(peer_id.to_string());
								}
								
								if !peer_ids.is_empty() {
									let _ = self.event_sender
										.send(Event::PingEligiblePeers {
											epoch: 0, // Or determine current epoch
											peer_ids
										})
										.await
										.unwrap_or_else(|e| error!("Failed to send PingEligiblePeers event: {:?}", e));
									
								}	
							}
							Err(e) => {
								warn!("GetClosestPeers query failed: {:?}", e);
							}
						}
						_ => info!("Some OutboundQueryProgressed event received: {result:?}"),
					},
					KademliaEvent::RoutingUpdated { peer, .. } => {
						info!("Kademlia routing updated: {peer:?}")
					},
					_ => info!("Some Kademlia event received: {event:?}"),
				},
				BehaviourEvent::Gossipsub(event) => match event {
					gossipsub::Event::Subscribed { peer_id, topic } => {
						info!("Peer: {peer_id:?} subscribed to '{topic:?}'");
						match self.add_active_peer(peer_id).await {
							Ok(_) => debug!("Peer: {peer_id:?} added to ACTIVE_PEERS"),
							Err(e) => error!("Peer: {peer_id:?} failed to add to ACTIVE_PEERS: {:?}", e),
						}
					},
					gossipsub::Event::Unsubscribed { peer_id, topic } => {
						info!("Peer: {peer_id:?} unsubscribed from '{topic:?}'");
						match self.remove_active_peer(peer_id).await {
							Ok(_) => debug!("Peer: {peer_id:?} removed from ACTIVE_PEERS"),
							Err(e) => error!("Peer: {peer_id:?} failed to remove from ACTIVE_PEERS: {:?}", e),
						}
					},
					gossipsub::Event::Message {
						propagation_source: peer_id,
						message_id: id,
						message,
					} => {
						// Are we recieving a transaction/block_payload/etc
						match message.topic.as_str() {
							NODE_INFO_TOPIC => {
								match deserialize_from_versioned_message::<NodeInfo>(&message.data) {
									Ok(node_info) => {
										let _ = self
											.event_sender
											.send(Event::InboundNodeInfo { node_info })
											.await
											.unwrap_or_else(|e| {
												error!(
													"Failed to send incoming node_info to receiver: {:?}",
													e
												)
											});
									},
									Err(e) => {
										warn!("Can't deserialize NodeInfo from peer {}: {}", peer_id, e)
									}									
								}
							},
							TRANSACTIONS_TOPIC => {
								match deserialize_from_versioned_message::<Transaction>(&message.data) {
									Ok(transaction) => {
										let _ = self
											.event_sender
											.send(Event::InboundTransaction { transaction })
											.await
											.unwrap_or_else(|e| {
												error!(
													"Failed to send incoming tx to receiver: {:?}",
													e
												)
											});
									},
									Err(e) => {
										warn!("Can't deserialize Transaction from peer {}: {}", peer_id, e)
									}
								}
							},
							BLOCKS_VALIDATE_TOPIC => {
								match deserialize_from_versioned_message::<BlockPayload>(&message.data) {
									Ok(block_payload) => {
										// Initialize eligible peers for ping results
										if block_payload.block.block_header.block_number == ELIGIBLE_PEERS_INIT_BLOCK_NUMBER {
											let mut peer_ids: Vec<String> = Vec::new();
											for bucket in self.swarm.behaviour_mut().kademlia.kbuckets() {
												for entry in bucket.iter() {
													let peer_id = entry.node.key.clone().into_preimage(); // Extract the PeerId
													peer_ids.push(peer_id.to_string());
												}
											}
											let _ = self
												.event_sender
												.send(Event::PingEligiblePeers {
													epoch: 0,
													peer_ids
												})
												.await
												.unwrap_or_else(|e| {
													error!(
														"Failed to send PingEligiblePeers event: {:?}",
														e
													)
												});
										}

										let _ = self
											.event_sender
											.send(Event::InboundValidateBlock { block_payload })
											.await
											.unwrap_or_else(|e| {
												error!(
													"Failed to send incoming block_payload validate to receiver: {:?}",
													e
												)
											});
									},
									Err(e) => {
										warn!("Can't deserialize BlockValidatePayload from peer {}: {}", peer_id, e)
									}
								}
							},
							BLOCK_PROPOSER_TOPIC => {
								match deserialize_from_versioned_message::<BlockProposerPayload>(&message.data) {
									Ok(block_proposer_payload) => {
										let _ = self
											.event_sender
											.send(Event::InboundBlockProposer { block_proposer_payload })
											.await
											.unwrap_or_else(|e| {
												error!(
													"Failed to send incoming block_proposer_payload to receiver: {:?}",
													e
												)
											});
									},
									Err(e) => {
										warn!("Can't deserialize BlockProposerPayload from peer {}: {}", peer_id, e)
									}
								}
							},
							VOTE_TOPIC => {
								match deserialize_from_versioned_message::<Vote>(&message.data) {
									Ok(vote) => {
										let _ = self
											.event_sender
											.send(Event::InboundVote { vote })
											.await
											.unwrap_or_else(|e| {
												error!(
													"Failed to send incoming vote to receiver: {:?}",
													e
												)
											});
									},
									Err(e) => {
										warn!("Can't deserialize Vote: {}", e)
									}
								}
							},
							VOTE_RESULT_TOPIC => {
								match deserialize_from_versioned_message::<VoteResult>(&message.data) {
									Ok(vote_result) => {
										let _ = self
											.event_sender
											.send(Event::InboundVoteResult { vote_result })
											.await
											.unwrap_or_else(|e| {
												error!(
													"Failed to send incoming vote_result to receiver: {:?}",
													e
												)
											});
									},
									Err(e) => {
										warn!("Can't deserialize VoteResult from peer {}: {}", peer_id, e)
									}
								}
							},
							NODE_HEALTH_TOPIC => {
								match deserialize_from_versioned_message::<Vec<NodeHealth>>(&message.data) {
									Ok(node_healths) => {
										let _ = self
											.event_sender
											.send(Event::InboundNodeHealth { node_healths })
											.await
											.unwrap_or_else(|e| {
												error!(
													"Failed to send incoming node_health to receiver: {:?}",
													e
												)
											});
									},
									Err(e) => {
										warn!("Can't deserialize NodeHealth from peer {}: {}", peer_id, e)
									}
								}
							},
							AGGREGATED_NODE_HEALTH_TOPIC => {
								match deserialize_from_versioned_message::<Vec<NodeHealthPayload>>(&message.data) {
									Ok(aggregated_healths) => {
										let _ = self
											.event_sender
											.send(Event::InboundAggregatedNodeHealth { aggregated_healths })
											.await
											.unwrap_or_else(|e| {
												error!(
													"Failed to send incoming aggregated_node_health to receiver: {:?}",
													e
												)
											});
									},
									Err(e) => {
										warn!("Can't deserialize Aggregated NodeHealth from peer {}: {}", peer_id, e)
									}
								}
							},
							_ => {
								warn!("Topic {} not supported. Shouldn't recieve an unknown or un-subscribed from topic.", message.topic.as_str());
							},
						}
						debug!(
							"New p2p message received: '{}' with id: {id} from peer: {peer_id}",
							String::from_utf8_lossy(&message.data),
						)
					},
					_ => info!("Some Gossipsub event received: {event:?}"),
				},
				BehaviourEvent::AutoNat(event) => match event {
					autonat::Event::InboundProbe(inbound_event) => match inbound_event {
						autonat::InboundProbeEvent::Error { peer, error, .. } => {
							debug!(
								"AutoNAT Inbound Probe failed with Peer: {}. Error: {:#?}.",
								peer, error
							);
						}
						_ => {
							log::trace!("AutoNAT Inbound Probe: {:#?}", inbound_event);
						}
					},
					autonat::Event::OutboundProbe(outbound_event) => match outbound_event {
						autonat::OutboundProbeEvent::Error { peer, error, .. } => {
							debug!(
								"AutoNAT Outbound Probe failed with Peer: {:#?}. Error: {:#?}",
								peer, error
							);
						}
						_ => {
							log::trace!("AutoNAT Outbound Probe: {:#?}", outbound_event);
						}
					},
					autonat::Event::StatusChanged { old, new } => {
						debug!(
							"AutoNAT Old status: {:#?}. AutoNAT New status: {:#?}",
							old, new
						);
						let local_peer_id = self.swarm.local_peer_id().clone();
						let behaviour = self.swarm.behaviour_mut();
						match new {
							autonat::NatStatus::Public(addr) => {
								// Log the discovery of a new public address
								info!("Discovered public address: {}", addr);
								behaviour.kademlia.add_address(&local_peer_id, addr.clone());
								// Share public address with other nodes
								behaviour.kademlia.get_closest_peers(local_peer_id);
								info!("Added public address {} for peer {}", addr, local_peer_id);
							}
							autonat::NatStatus::Private => {
								if let Some(addr) = match old {
									autonat::NatStatus::Public(addr) => Some(addr),
									_ => None,
								} {
									warn!("Peer changed to private or unknown address");
									// Remove peer from the routing table and address_peers
									behaviour.kademlia.remove_address(&local_peer_id, &addr);
									info!("Removed address {} for peer {}", addr, local_peer_id);
								}
							}
							autonat::NatStatus::Unknown => {
								info!("Peer address is unknown")
							}
						}
					}
				},
				BehaviourEvent::RequestResponse(event) => match event {
					request_response::Event::Message { peer: sender_peer, message } => match message {
						request_response::Message::Request { request, channel, .. } => {
							log::debug!("Received request: {:?} from channel: {:?}", request, channel);
							match request {
								L1xRequest::QueryBlock(request) => {
									self.event_sender
										.send(Event::InboundQueryBlockRequest { request, channel })
										.await
										.unwrap_or_else(|e| {
											error!(
												"Failed to send incoming query block request to receiver: {:?}",
												e
											)
										})
								},
								L1xRequest::QueryNodeStatus(request_time) => {
									let local_peer_id = self.swarm.local_peer_id().clone();
									let response =  L1xResponse::QueryNodeStatus(local_peer_id.to_string(), request_time);
									self.swarm.behaviour_mut().request_response.send_response(channel, response)
										.unwrap_or_else(|e| {
											error!(
												"Failed to send query status response to receiver: {:?}",
												e
											)
										});
								}
							}
						},
						request_response::Message::Response { response, .. } => {
							log::debug!("Received response: {:?}", response);
							match response {
								L1xResponse::QueryBlock { block_payload, is_finalized, vote_result } => {
									self.event_sender
										.send(Event::InboundQueryBlockResponse { block_payload, is_finalized, vote_result })
										.await
										.unwrap_or_else(|e| {
											error!(
												"Failed to send incoming query block response to receiver: {:?}",
												e
											)
										})
								}
								L1xResponse::QueryBlockError(error_message) => {
									log::warn!("Query block error: {:?}", error_message);
								}

								L1xResponse::QueryNodeStatus(_, request_time) => {
									let peer_id = sender_peer.to_string(); // Use the actual sender peer id
									log::debug!("Received L1xResponse::QueryNodeStatus response from peer: {:?}", peer_id);
									if let Ok(current_timestamp) = util::generic::current_timestamp_in_millis() {
										let response_time = current_timestamp - request_time;
										log::debug!("Sending Event::PingResult event to event_sender channel, peer_id: {:?}, is_success: {:?}, rtt: {:?}", peer_id, true, response_time as u64);
										self.event_sender.send(Event::PingResult {
											peer_id: peer_id,
											is_success: true,
											rtt: response_time as u64
										})
											.await
											.unwrap_or_else(|e| {
												error!(
												"Failed to send incoming query status response to receiver: {:?}",
												e
											)
											})
									} else {
										error!("Failed to get current timestamp in L1xResponse")
									}
								}
								// Add other response types
							}
						}
					},
					_ => {},
				},

			},
			e => warn!("Behaviour: {e:?}"),
		}
	}

	/// Given a vallid `Command`, execute the proper underlying libp2p calls
	async fn handle_command(&mut self, command: Command) {
		debug!("MADE IT TO HANDLE_COMMAND()");
		match command {
			Command::StartListening { addr, sender } => {
				let _ = match self.swarm.listen_on(addr) {
					Ok(_) => sender.send(Ok(())),
					Err(e) => sender.send(Err(Box::new(e))),
				};
			},
			Command::Dial { peer_id, peer_addr, sender } => {
				if let hash_map::Entry::Vacant(e) = self.pending_dial.entry(peer_id) {
					match self.swarm.dial(peer_addr.clone().with(Protocol::P2p(peer_id.into()))) {
						Ok(()) => {
							e.insert(sender);

							self.swarm
								.behaviour_mut()
								.kademlia
								// .add_address(&peer_id, "/dnsaddr/bootstrap.libp2p.io".parse()?);
								.add_address(&peer_id, peer_addr);
						},
						Err(e) => {
							let _ = sender.send(Err(Box::new(e)));
						},
					}
				} else {
					todo!("Already dialing peer.");
				}
			},
			Command::BroadcastNodeInfo { node_info, sender } => match serialize_as_versioned_message(node_info) {
				Ok(tx_bytes) => match self
					.swarm
					.behaviour_mut()
					.gossipsub
					.publish(gossipsub::IdentTopic::new(NODE_INFO_TOPIC), tx_bytes)
				{
					Ok(msg_id) => {
						info!("NodeInfo MESSAGE PUBLISHED SUCCESSFULLY");
						let _ = sender.send(Ok(msg_id));
					},
					Err(e) => {
						// error!("FAILED TO PUBLISH NodeInfo MESSAGE ");
						let _ = sender.send(Err(Box::new(e)));
					},
				},
				Err(e) => {
					error!("FAILED TO SERIALIZE TRANSACTION TO BYTES");
					let _ = sender.send(Err(e.into()));
				},
			},
			Command::BroadcastTransaction { transaction, sender } => match serialize_as_versioned_message(transaction) {
				Ok(tx_bytes) => match self
					.swarm
					.behaviour_mut()
					.gossipsub
					.publish(gossipsub::IdentTopic::new(TRANSACTIONS_TOPIC), tx_bytes)
				{
					Ok(msg_id) => {
						info!("TRANSACTION PUBLISHED SUCCESSFULLY");
						let _ = sender.send(Ok(msg_id));
					},
					Err(e) => {
						// error!("FAILED TO PUBLISH MESSAGE");
						let _ = sender.send(Err(Box::new(e)));
					},
				},
				Err(e) => {
					error!("FAILED TO SERIALIZE TRANSACTION TO BYTES");
					let _ = sender.send(Err(e.into()));
				},
			},
			Command::BroadcastValidateBlock { block_payload, sender } => {
				match serialize_as_versioned_message(block_payload) {
					Ok(block_bytes) => match self
						.swarm
						.behaviour_mut()
						.gossipsub
						.publish(gossipsub::IdentTopic::new(BLOCKS_VALIDATE_TOPIC), block_bytes)
					{
						Ok(msg_id) => {
							info!("VALIDATE BLOCK PUBLISHED SUCCESSFULLY");
							let _ = sender.send(Ok(msg_id));
						},
						Err(e) => {
							// error!("FAILED TO PUBLISH BLOCK");
							let _ = sender.send(Err(Box::new(e)));
						},
					},
					Err(e) => {
						error!("FAILED TO SERIALIZE BLOCK TO BYTES");
						let _ = sender.send(Err(e.into()));
					},
				}
			},
			Command::BroadcastBlockProposer { block_proposer_payload, sender } =>
				match serialize_as_versioned_message(block_proposer_payload) {
					Ok(cluster_block_proposers_bytes) => {
						match self.swarm.behaviour_mut().gossipsub.publish(
							gossipsub::IdentTopic::new(BLOCK_PROPOSER_TOPIC),
							cluster_block_proposers_bytes,
						) {
							Ok(msg_id) => {
								info!("BLOCK PROPOSER PUBLISHED SUCCESSFULLY");
								let _ = sender.send(Ok(msg_id));
							},
							Err(e) => {
								// error!("FAILED TO PUBLISH BLOCK PROPOSER");
								let _ = sender.send(Err(Box::new(e)));
							},
						}
					},
					Err(e) => {
						error!("FAILED TO SERIALIZE BLOCK PROPOSER TO BYTES");
						let _ = sender.send(Err(e.into()));
					},
				},
			Command::BroadcastVote { vote, sender } => match serialize_as_versioned_message(vote) {
				Ok(cluster_vote_bytes) => {
					match self
						.swarm
						.behaviour_mut()
						.gossipsub
						.publish(gossipsub::IdentTopic::new(VOTE_TOPIC), cluster_vote_bytes)
					{
						Ok(msg_id) => {
							info!("VOTE PUBLISHED SUCCESSFULLY");
							let _ = sender.send(Ok(msg_id));
						},
						Err(e) => {
							// error!("FAILED TO PUBLISH VOTE");
							let _ = sender.send(Err(Box::new(e)));
						},
					}
				},
				Err(e) => {
					error!("FAILED TO SERIALIZE VOTE TO BYTES");
					let _ = sender.send(Err(e.into()));
				},
			},
			Command::BroadcastVoteResult { vote_result, sender } => match serialize_as_versioned_message(vote_result) {
				Ok(cluster_vote_result_bytes) => {
					match self.swarm.behaviour_mut().gossipsub.publish(
						gossipsub::IdentTopic::new(VOTE_RESULT_TOPIC),
						cluster_vote_result_bytes,
					) {
						Ok(msg_id) => {
							info!("VOTE RESULT PUBLISHED SUCCESSFULLY");
							let _ = sender.send(Ok(msg_id));
						},
						Err(e) => {
							// error!("FAILED TO PUBLISH VOTE RESULT");
							let _ = sender.send(Err(Box::new(e)));
						},
					}
				},
				Err(e) => {
					error!("FAILED TO SERIALIZE VOTE RESULT TO BYTES");
					let _ = sender.send(Err(e.into()));
				},
			},
			Command::BroadcastNodeHealth { node_healths, sender } => {
				match serialize_as_versioned_message(node_healths.clone()) {
					Ok(healths) => {
						match self.swarm.behaviour_mut().gossipsub.publish(
							gossipsub::IdentTopic::new(NODE_HEALTH_TOPIC),
							healths,
						) {
							Ok(msg_id) => {
								info!("NODE HEALTHS PUBLISHED SUCCESSFULLY");
								let _ = sender.send(Ok(msg_id));
							},
							Err(e) => {
								// error!("FAILED TO PUBLISH NODE HEALTH");
								let _ = sender.send(Err(Box::new(e)));
							},
						}
					},
					Err(e) => {
						error!("FAILED TO SERIALIZE NODE HEALTH TO BYTES");
						let _ = sender.send(Err(e.into()));
					},
				}

				// Initialize eligible peers for ping results
				// let mut peer_ids: Vec<String> = Vec::new();
				// for bucket in self.swarm.behaviour_mut().kademlia.kbuckets() {
				// 	for entry in bucket.iter() {
				// 		let peer_id = entry.node.key.clone().into_preimage(); // Extract the PeerId
				// 		peer_ids.push(peer_id.to_string());
				// 	}
				// }
				// if let Some(epoch) = node_healths.first().map(|node_health| node_health.epoch) {
				// 	let _ = self
				// 		.event_sender
				// 		.send(Event::PingEligiblePeers {
				// 			epoch: epoch + 1, // Setting peer_ids for next epoch
				// 			peer_ids
				// 		})
				// 		.await
				// 		.unwrap_or_else(|e| {
				// 			error!(
				// 			"Failed to send PingEligiblePeers event: {:?}",
				// 			e
				// 		)
				// 		});
				// }
			},
            Command::BroadcastNodeHealthPayload { node_health_payloads, sender } => match serialize_as_versioned_message(node_health_payloads) {
				Ok(health) => {
					match self.swarm.behaviour_mut().gossipsub.publish(
						gossipsub::IdentTopic::new(AGGREGATED_NODE_HEALTH_TOPIC),
						health,
					) {
						Ok(msg_id) => {
							info!("NODE HEALTH PAYLOAD PUBLISHED SUCCESSFULLY");
							let _ = sender.send(Ok(msg_id));
						},
						Err(e) => {
							// error!("FAILED TO PUBLISH NODE HEALTH PAYLOAD");
							let _ = sender.send(Err(Box::new(e)));
						},
					}
				},
				Err(e) => {
					error!("FAILED TO SERIALIZE NODE HEALTH PAYLOAD TO BYTES");
					let _ = sender.send(Err(e.into()));
				},
			},
			Command::TopicSubscribe { topic_string, sender } => {
				let topic = gossipsub::IdentTopic::new(topic_string.clone());
				match self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
					Ok(_) => {
						let _ = sender.send(Ok(()));
					},
					Err(e) => {
						error!("Failed to subscribe to topic {topic_string:?}: {e:?}");
						let _ = sender.send(Err(Box::new(e)));
					},
				}
			},
			Command::TopicUnsubscribe { topic_string, sender } => {
				let topic = gossipsub::IdentTopic::new(topic_string.clone());
				match self.swarm.behaviour_mut().gossipsub.unsubscribe(&topic) {
					Ok(_) => {
						let _ = sender.send(Ok(()));
					},
					Err(e) => {
						error!("Failed to unsubscribe from topic {topic_string:?}: {e:?}");
						let _ = sender.send(Err(Box::new(e)));
					},
				}
			},
			Command::QueryBlockRequest {peer_id, request, sender} =>{
				let request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, request);
				let _ = sender.send(Ok(request_id));
			},
			Command::QueryBlockResponse { channel, response, sender } => {
				match self.swarm.behaviour_mut().request_response.send_response(channel, response) {
					Ok(_) => {
						let _ = sender.send(Ok(()));
					},
					Err(e) => {
						error!("Failed to send response for request-response protocol{:?}", e);
						let _ = sender.send(Err(Box::new(io::Error::new(io::ErrorKind::Other, "Send response error"))));
					}
				}
			},
			Command::QueryStatusRequest {peer_id, request, sender} =>{
				let request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, request);
				let _ = sender.send(Ok(request_id));
			},
		}
	}

	async fn handle_peer_disconnection(&mut self, disconnected_peer_id: PeerId) {
		self.swarm.behaviour_mut().kademlia.get_closest_peers(disconnected_peer_id);
	}			
		
}

/// Our network behaviour.
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "BehaviourEvent")]
struct Behaviour {
	identify: identify::Behaviour,
	// mdns: mdns::tokio::Behaviour,
	kademlia: Kademlia<MemoryStore>,
	gossipsub: gossipsub::Behaviour,
	auto_nat: autonat::Behaviour,
	request_response: request_response::Behaviour<L1xCodec>,
}

#[derive(Clone)]
struct L1xCodec;

// Add Maximum size limit for request
impl L1xCodec {
	const MAX_REQUEST_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
	const MAX_RESPONSE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
}

#[derive(Debug, Clone)]
struct L1xProtocol;

impl ProtocolName for L1xProtocol {
	fn protocol_name(&self) -> &[u8] {
		"/l1x/protocol/1.0.0".as_bytes()
	}
}

#[async_trait]
impl libp2p::request_response::Codec for L1xCodec {
	type Protocol = L1xProtocol;
	type Request = L1xRequest;
	type Response = L1xResponse;

	async fn read_request<T>(
		&mut self,
		_: &Self::Protocol,
		socket: &mut T,
	) -> io::Result<Self::Request>
	where
		T: AsyncRead + Unpin + Send,
	{
		let mut response = Vec::new();
		// Add size limit for request
		let mut socket = socket.take(Self::MAX_REQUEST_SIZE);
		socket.read_to_end(&mut response).await?;
		deserialize(&response)
	}

	async fn read_response<T>(
		&mut self,
		_: &Self::Protocol,
		socket: &mut T,
	) -> io::Result<Self::Response>
	where
		T: AsyncRead + Unpin + Send,
	{
		let mut response = Vec::new();
		// Add size limit for response
		let mut socket = socket.take(Self::MAX_RESPONSE_SIZE);
		socket.read_to_end(&mut response).await?;
		deserialize(&response)
	}

	async fn write_request<T>(
		&mut self,
		_protocol: &Self::Protocol,
		socket: &mut T,
		req: Self::Request,
	) -> io::Result<()>
	where
		T: AsyncWrite + Unpin + Send,
	{
		let encoded_data = serialize(&req)?;
		socket.write_all(&encoded_data).await?;
		Ok(())
	}

	async fn write_response<T>(
		&mut self,
		_protocol: &Self::Protocol,
		socket: &mut T,
		res: Self::Response,
	) -> io::Result<()>
	where
		T: AsyncWrite + Unpin + Send,
	{
		let encoded_data = serialize(&res)?;
		socket.write_all(&encoded_data).await?;
		Ok(())
	}
}

fn kademlia_behaviour(local_peer_id: PeerId) -> Kademlia<MemoryStore> {
	let mut config = KademliaConfig::default();
	config.set_query_timeout(Duration::from_secs(5 * 60));
	let store = MemoryStore::new(local_peer_id);
	Kademlia::with_config(local_peer_id, store, config)
}

fn autonat_behaviour(local_peer_id: PeerId, autonat_config: &Option<system::config::AutonatConfig>) -> autonat::Behaviour {
	let mut config = autonat::Config::default();

	if let Some(autonat_config) = autonat_config {
		if let Some(timeout) = autonat_config.timeout {
			config.timeout = Duration::from_secs(timeout);
		}
		if let Some(boot_delay) = autonat_config.boot_delay {
			config.boot_delay = Duration::from_secs(boot_delay);
		}
		if let Some(refresh_interval) = autonat_config.refresh_interval {
			config.refresh_interval = Duration::from_secs(refresh_interval);
		}
		if let Some(retry_interval) = autonat_config.retry_interval {
			config.retry_interval = Duration::from_secs(retry_interval);
		}
		if let Some(throttle_server_period) = autonat_config.throttle_server_period {
			config.throttle_server_period = Duration::from_secs(throttle_server_period);
		}
		if let Some(confidence_max) = autonat_config.confidence_max {
			config.confidence_max = confidence_max;
		}
		if let Some(max_peer_addresses) = autonat_config.max_peer_addresses {
			config.max_peer_addresses = max_peer_addresses;
		}
		if let Some(throttle_clients_global_max) = autonat_config.throttle_clients_global_max {
			config.throttle_clients_global_max = throttle_clients_global_max;
		}
		if let Some(throttle_clients_peer_max) = autonat_config.throttle_clients_peer_max {
			config.throttle_clients_peer_max = throttle_clients_peer_max;
		}
		if let Some(throttle_clients_period) = autonat_config.throttle_clients_period {
			config.throttle_clients_period = Duration::from_secs(throttle_clients_period);
		}
		if let Some(only_global_ips) = autonat_config.only_global_ips {
			config.only_global_ips = only_global_ips;
		}
	}

	autonat::Behaviour::new(
		local_peer_id,
		config,
	)
}

fn gossipsub_behaviour(
	local_key: identity::Keypair,
	topics: Vec<gossipsub::IdentTopic>,
) -> Result<gossipsub::Behaviour, Box<dyn Error>> {
	// To content-address message, we can take the hash of message and use it as an ID.
	let message_id_fn = |message: &gossipsub::Message| {
		let mut s = DefaultHasher::new();
		message.data.hash(&mut s);
		gossipsub::MessageId::from(s.finish().to_string())
	};

	let gossipsub_config = gossipsub::ConfigBuilder::default()
		.heartbeat_interval(Duration::from_secs(4))
		.validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
		.message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated. NOTE:
		// Not sure if we want this method or a different method
		.max_transmit_size(100_000_000)
		.build()
		.expect("Valid config");

	let mut gossipsub = gossipsub::Behaviour::new(
		gossipsub::MessageAuthenticity::Signed(local_key),
		gossipsub_config,
	)
	.expect("Correct configuration");

	// Subscribe to the topics
	for topic in topics {
		gossipsub.subscribe(&topic)?;
	}

	Ok(gossipsub)
}

/// Valid commands that can be sent from the Client to the EventLoop
#[derive(Debug)]
pub enum Command {
	StartListening {
		addr: Multiaddr,
		sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
	},
	/// Dial a peer
	Dial {
		peer_id: PeerId,
		peer_addr: Multiaddr,
		sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
	},
	/// Broadcast an L1X transaction to the network
	BroadcastNodeInfo {
		node_info: NodeInfo,
		sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
	},
	/// Broadcast an L1X transaction to the network
	BroadcastTransaction {
		transaction: Transaction,
		sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
	},
	/// Broadcast a proposed block_payload to the network for validation
	BroadcastValidateBlock {
		block_payload: BlockPayload,
		sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
	},
	/// Broadcast a block_payload header to the network
	BroadcastBlockProposer {
		block_proposer_payload: BlockProposerPayload,
		sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
	},
	/// Broadcast a vote_payload to the network
	BroadcastVote {
		vote: Vote,
		sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
	},
	/// Broadcast a vote_payload to the network
	BroadcastVoteResult {
		vote_result: VoteResult,
		sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
	},
	/// Subscribe to a gossipsub topic
	TopicSubscribe {
		topic_string: String,
		sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
	},
	/// Unsubscribe from a gossipsub topic
	TopicUnsubscribe {
		topic_string: String,
		sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
	},

	/// Queryblock to the network
	QueryBlockRequest {
		peer_id: PeerId,
		request: L1xRequest,
		sender: oneshot::Sender<Result<RequestId, Box<dyn Error + Send>>>,
	},

	/// Queryblock response
	QueryBlockResponse {
		channel: ResponseChannel<L1xResponse>,
		response: L1xResponse,
		sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
	},

	/// Broadcast local node health
    BroadcastNodeHealth {
        node_healths: Vec<NodeHealth>,
        sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
    },

	/// broadcast signed node health
   BroadcastNodeHealthPayload {
		node_health_payloads: Vec<NodeHealthPayload>,
        sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
    },
	/// Queryblock to the network
	QueryStatusRequest {
		peer_id: PeerId,
		request: L1xRequest,
		sender: oneshot::Sender<Result<RequestId, Box<dyn Error + Send>>>,
	},
}

/// Events that are supported to send to the event_receiver for processing
/// ex: Receiving a new transaction from a peer node so it is sent to the event_receiver
/// where it is then validated and added to the mempool.
#[derive(Debug)]
pub enum Event {
	InboundNodeInfo { node_info: NodeInfo },
	InboundTransaction { transaction: Transaction },
	InboundValidateBlock { block_payload: BlockPayload },
	InboundBlockProposer { block_proposer_payload: BlockProposerPayload },
	InboundVote { vote: Vote },
	InboundVoteResult { vote_result: VoteResult },
	InboundQueryBlockResponse { block_payload: BlockPayload, is_finalized: bool, vote_result: Option<VoteResult> },
	InboundQueryBlockRequest { request: QueryBlockMessage, channel: ResponseChannel<L1xResponse> },
	AggregateNodeHealth { epoch: Epoch },
	ProcessNodeHealth { epoch: Epoch },
	InboundNodeHealth { node_healths: Vec<NodeHealth> },
	InboundAggregatedNodeHealth { aggregated_healths: Vec<NodeHealthPayload> },
	PingResult { peer_id: String, is_success: bool, rtt: u64 },
	PingEligiblePeers { epoch: Epoch, peer_ids: Vec<String> },
	CheckNodeStatus,
	InitializePeers,
}

/// Given a human-readable topic name, return the topic hash
fn get_topic_hash(topic_str: &str) -> gossipsub::IdentTopic {
	gossipsub::IdentTopic::new(topic_str)
}

fn is_public_address(addr: &Multiaddr) -> bool {
	for protocol in addr.iter() {
		match protocol {
			libp2p::multiaddr::Protocol::Ip4(ip) => {
				if ip.is_private() || ip.is_loopback() || ip.is_multicast() || ip.is_unspecified() {
					return false;
				}
			}
			libp2p::multiaddr::Protocol::Ip6(ip) => {
				if ip.is_loopback() || ip.is_multicast() || ip.is_unspecified() {
					return false;
				}
			}
			_ => {}
		}
	}
	true
}
