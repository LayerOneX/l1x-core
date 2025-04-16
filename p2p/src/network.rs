use crate::config::*;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use itertools::Itertools;
use libp2p::multiaddr::Protocol;
use libp2p::{autonat, futures::stream::FuturesUnordered, futures::StreamExt, swarm::ConnectionHandlerUpgrErr};
use libp2p::{
    core::upgrade,
    identify,
    kad::{record::store::MemoryStore, Kademlia, KademliaConfig, KademliaEvent, QueryResult},
};
use libp2p::{
    identity,
    identity::Keypair,
    noise,
    request_response::{self, *},
    swarm::{NetworkBehaviour, Swarm, SwarmBuilder, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Transport,
};
use libp2p_gossipsub::{self as gossipsub, MessageId};
use log::{debug, error, info, warn};
use primitives::{Address, Epoch};
use system::config::MpscConfig;
use std::io;
use std::{
    collections::{
        hash_map::{self, DefaultHasher},
        HashMap,
    },
    str::FromStr,
    sync::Arc,
};
use std::{
    error::Error,
    hash::{Hash, Hasher},
};
use void::Void;

use block::block_manager::{BlockManager, BlockManagerCache};
use compile_time_config::ELIGIBLE_PEERS_INIT_BLOCK_NUMBER;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;
use system::node_health::NodeHealth;
use system::{
    block::{
        BlockBroadcast, BlockPayload, BlockQueryRequest, L1xRequest, L1xResponse, QueryBlockMessage,
        QueryBlockResponse, QueryStatusRequest,BroadcastNodeDetailedStatus
    },
    block_proposer::{BlockProposerBroadcast, BlockProposerPayload},
    dht_health_storage::DHTHealthStorage,
    node_health::{AggregatedNodeHealthBroadcast, NodeHealthBroadcast, NodeHealthPayload},
    node_info::{NodeInfo, NodeInfoBroadcast},
    protocol::{deserialize_from_versioned_message, serialize_as_versioned_message},
    transaction::{Transaction, TransactionBroadcast},
    vote::{Vote, VoteBroadcast},
    vote_result::{VoteResult, VoteResultBroadcast},
	node_status::NodeDetailedStatus
};
use tokio::sync::{mpsc, oneshot};
use parking_lot::RwLock;

fn deserialize<'a, R: Deserialize<'a>>(encoded_data: &'a [u8]) -> Result<R, io::Error> {
    bincode::deserialize(encoded_data).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

fn serialize<D: Serialize>(data: &D) -> Result<Vec<u8>, io::Error> {
    bincode::serialize(data).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

const MAX_CLOSEST_PEERS: usize = 10;

lazy_static! {
    pub static ref GLOBAL_NETWORK_STATE: Arc<NetworkState> = Arc::new(NetworkState::new_internal());
}

#[derive(Debug, Clone)]
pub struct PeerStatusInfo {
    pub current_block: Option<primitives::BlockNumber>,
    pub pending_transactions: Option<u32>,
    pub connected_peers: Option<u32>,
    pub uptime_seconds: Option<u64>,
    pub last_update_time: Option<u64>,
}

impl Default for PeerStatusInfo {
    fn default() -> Self {
        Self {
            current_block: Some(0),
            pending_transactions: Some(0),
            connected_peers: Some(0),
            uptime_seconds: Some(0),
            last_update_time: Some(0),
        }
    }
}

#[derive(Debug)]
pub struct NetworkState {
    available_peers_by_address: RwLock<HashMap<Address, PeerId>>,
    available_peers_by_peer_id: RwLock<HashMap<PeerId, Address>>,
    available_peers_list_by_peer_id: RwLock<HashSet<PeerId>>,
    peer_status_info: RwLock<HashMap<PeerId, PeerStatusInfo>>,
}

pub struct NetworkStateActivePeer {
    pub peer_id: PeerId,
    pub address: Address,
    pub peer_status_info: PeerStatusInfo,
}

impl NetworkState {
    fn new_internal() -> Self {
        Self {
            available_peers_by_address: RwLock::new(HashMap::new()),
            available_peers_by_peer_id: RwLock::new(HashMap::new()),
            available_peers_list_by_peer_id: RwLock::new(HashSet::new()),
            peer_status_info: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_instance() -> &'static Arc<Self> {
        &GLOBAL_NETWORK_STATE
    }

    pub async fn get_connected_peers_count(&self) -> usize {
        self.available_peers_list_by_peer_id.read().len()
    }

    pub async fn has_connected_peers(&self) -> bool {
        !self.available_peers_list_by_peer_id.read().is_empty()
    }

    pub async fn is_peer_available(&self, node_address: &Address) -> bool {
        self.available_peers_by_address.read().contains_key(node_address)
    }

    pub async fn add_active_peer(&self, peer_id: PeerId) -> Result<(), Box<dyn Error + Send>> {
        debug!("🔍 Network - Connection Established | Adding peer: {:?}", peer_id);

        // Add peer to the list of available peers
        {
            let mut available_peers_list = self.available_peers_list_by_peer_id.write();
            if !available_peers_list.contains(&peer_id) {
                available_peers_list.insert(peer_id);
            }
        }

        // Add peer to the list of available peers with status info
        {
            let mut peer_status_info = self.peer_status_info.write();
            if !peer_status_info.contains_key(&peer_id) {
                peer_status_info.insert(peer_id, PeerStatusInfo::default());
            }
        }

        Ok(())
    }

    pub async fn add_active_peers(&self, peers: Vec<PeerId>) -> Result<(), Box<dyn Error + Send>> {
        for peer_id in peers {
            let _ = self.add_active_peer(peer_id).await;
        }
        Ok(())
    }

    pub async fn remove_active_peer(&self, peer_id: PeerId) -> Result<(), Box<dyn Error + Send>> {
        debug!("🔍 Network - Connection Closed | Removing peer: {:?}", peer_id);

        let peers_exists = {
            let mut available_peers_list = self.available_peers_list_by_peer_id.write();
            available_peers_list.remove(&peer_id)
        };

        if peers_exists {
            let peer_address = {
                let available_peers_by_peer_id = self.available_peers_by_peer_id.read();
                available_peers_by_peer_id.get(&peer_id).cloned().unwrap_or_default()
            };

            if peer_address != Address::default() {
                let mut available_peers_by_address = self.available_peers_by_address.write();
                available_peers_by_address.remove(&peer_address);
            }

            {
                let mut peer_status_info = self.peer_status_info.write();
                peer_status_info.remove(&peer_id);
            }
        }

        Ok(())
    }

    pub async fn get_available_peers(&self) -> Result<Vec<PeerId>, Box<dyn Error + Send>> {
        let available_peers_list = self.available_peers_list_by_peer_id.read();
        Ok(available_peers_list.iter().map(|peer_id| *peer_id).collect())
    }

    pub async fn get_available_peers_with_info(&self) -> Result<Vec<NetworkStateActivePeer>, Box<dyn Error + Send>> {
        let available_peers_list = self.available_peers_list_by_peer_id.read();
        let available_peers_by_peer_id = self.available_peers_by_peer_id.read();
        let peer_status_info = self.peer_status_info.read();
        
        let mut available_peers_with_info: Vec<NetworkStateActivePeer> = Vec::new();
        for peer_id in available_peers_list.iter() {
            let address = available_peers_by_peer_id.get(peer_id).cloned().unwrap_or_default();
            let peer_status_info = peer_status_info.get(peer_id).cloned().unwrap_or_default();
            
            available_peers_with_info.push(NetworkStateActivePeer {
                peer_id: *peer_id,
                address: address.clone(),
                peer_status_info,
            });
        }
        Ok(available_peers_with_info)
    }

    pub async fn update_active_peer_address(&self, peer_id: PeerId, address: Address) -> Result<(), Box<dyn Error + Send>> {
        debug!("🔍 Network - Connection Established | Updating address {:?} for peer: {:?}", address, peer_id);
        
        {
            let mut available_peers_by_peer_id = self.available_peers_by_peer_id.write();
            available_peers_by_peer_id.insert(peer_id, address);
        }

        {
            let mut available_peers_by_address = self.available_peers_by_address.write();
            available_peers_by_address.insert(address, peer_id);
        }
        Ok(())
    }

    pub async fn update_active_peer_status_info(&self, peer_id: PeerId, status_info: PeerStatusInfo) -> Result<(), Box<dyn Error + Send>> {
        if self.available_peers_list_by_peer_id.read().contains(&peer_id) {
            {
                let mut peer_status_info = self.peer_status_info.write();
                peer_status_info.insert(peer_id, status_info.clone());
            }
            
            debug!("🔍 Network - Connection Established | Updated status info for peer: {:?}, status info: {:?}", peer_id, status_info);
        } else {
            warn!("⚠️ Network - Connection Established | Peer not found: {:?}", peer_id);
        }
        Ok(())
    }

    pub async fn get_peer_status_info(&self, peer_id: PeerId) -> Option<PeerStatusInfo> {
        let peer_status_info = self.peer_status_info.read();
        match peer_status_info.get(&peer_id) {
            Some(peer_status_info) => Some(peer_status_info.clone()),
            None => {
                warn!(
                    "⚠️ Network - Connection Established | PeerId not found in peer_status_info: {:?}",
                    peer_id
                );
                None
            }
        }
    }
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
    eth_chain_id: Option<u64>,
    cluster_address: Option<String>,
	mpsc_channel_capacity: MpscConfig,
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

    // Create topic hashes using network information if available
    // let topic_prefix = if let (Some(chain_id), Some(cluster)) = (eth_chain_id, &cluster_address) {
    //     // We truncate cluster_address to 8 chars to keep topic name reasonable length
    //     let cluster_short = &cluster[0..std::cmp::min(8, cluster.len())];
    //     format!("l1x/{}/{}", chain_id, cluster_short)
    // } else if let Some(chain_id) = eth_chain_id {
    //     format!("l1x/{}", chain_id)
    // } else {
    //     "l1x".to_string()
    // };

    // Create new topic for transactions
    let node_join_topic = get_topic_hash(NODE_INFO_TOPIC);
    let tx_topic = get_topic_hash(TRANSACTIONS_TOPIC);
    let block_validate_topic = get_topic_hash(BLOCKS_VALIDATE_TOPIC);
    let block_proposer_topic = get_topic_hash(BLOCK_PROPOSER_TOPIC);
    let vote_topic = get_topic_hash(VOTE_TOPIC);
    let vote_result_topic = get_topic_hash(VOTE_RESULT_TOPIC);
    let node_health_topic = get_topic_hash(NODE_HEALTH_TOPIC);
    let aggregated_node_health_topic = get_topic_hash(AGGREGATED_NODE_HEALTH_TOPIC);
	let broadcast_node_detailed_status_topic = get_topic_hash(BROADCAST_NODE_DETAILED_STATUS_TOPIC);

    let topics = vec![
        node_join_topic,
        tx_topic,
        vote_result_topic,
        block_validate_topic,
        block_proposer_topic,
        vote_topic,
        node_health_topic,
        aggregated_node_health_topic,
		broadcast_node_detailed_status_topic,
    ];

    // Only the very first node should start subscribed to these validator only topics
    // as it is hardcoded to be a validator from genesis
    // if bootnodes.is_empty() {
    // 	topics.push(block_validate_topic);
    // 	topics.push(block_proposer_topic);
    // 	topics.push(vote_topic);
    // }

    // Create network identifier with network information
    let network_id = "/ipfs/id/1.0.0/l1x".to_string();

    // Build the Swarm, connecting the lower layer transport logic with the
    // higher layer network behaviour logic.
    let swarm = {
        let mut behaviour = Behaviour {
            identify: identify::Behaviour::new(identify::Config::new(
                network_id,
                local_keys.public(),
            )),
            // mdns: mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?,
            kademlia: kademlia_behaviour(local_peer_id),
            gossipsub: gossipsub_behaviour(local_keys.clone(), topics)?,
            auto_nat: autonat_behaviour(local_peer_id, autonat_config),
            request_response: {
                debug!("🔍 Network - Protocol | Creating request_response behavior with chain_id: {:?}, cluster: {:?}", eth_chain_id, cluster_address);
                libp2p::request_response::Behaviour::new(
                    L1xCodec,
                    vec![(L1xProtocol::new(eth_chain_id, cluster_address.clone()), ProtocolSupport::Full)],
                    request_response::Config::default(),
                )
            },
        };

        // If provided, bootstrap routing table with bootnode(s)
        if !bootnodes.is_empty() {
            for bootnode in bootnodes {
                info!("🔌 Network - Bootstrapping | Bootstrapping to: {}", bootnode);
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

    let (command_sender, command_receiver) = mpsc::channel(mpsc_channel_capacity.command);
    let (event_sender, event_receiver) = mpsc::channel(mpsc_channel_capacity.event);
    let client = Client {
        sender: command_sender,
        event_sender: event_sender.clone(),
    };
    let event_loop = EventLoop::new(swarm, command_receiver, event_sender, dht_health_storage, eth_chain_id, cluster_address.clone());

    // Start peer discovery
    // match client.start_peer_discovery().await {
    // 	Ok(_) => {},
    // 	Err(e) => {
    // 		error!("Failed to start peer discovery: {:?}", e);
    // 	}
    // };

    // Start peer discovery

    Ok((client, event_receiver, event_loop))
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
            .send(Command::TopicSubscribe {
                topic_string: topic,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

    /// Command the node to unsubscribe from the gossipsub messages published under the given
    /// `topic`
    pub async fn unsubscribe_from_topic(&mut self, topic: String) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::TopicUnsubscribe {
                topic_string: topic,
                sender,
            })
            .await
            .expect("Command receiver not to be dropped.");
        receiver.await.expect("Sender not to be dropped.")
    }

	pub async fn init_node_monitoring(&self, cluster_address: Address, latest_epoch: Epoch, peer_id: PeerId) -> Result<(), Box<dyn Error + Send>> {

		debug!("🔍 Network - Node Monitoring | Initializing node monitoring");

		const NODE_HEALTH_MONITORING_LOOP_INTERVAL: u64 = 10;
		const NODE_STATUS_MONITORING_LOOP_INTERVAL: u64 = 12;
		const PING_ELIGIBLE_PEERS_LOOP_INTERVAL: u64 = 20;
		const NODE_DETAILS_STATUS_BROADCASTING_LOOP_INTERVAL: u64 = 30;

		match self.start_node_health_monitoring(NODE_HEALTH_MONITORING_LOOP_INTERVAL).await {
			Ok(_) => debug!("🔍 Network - Node Monitoring | Successfully started node health monitoring"),
			Err(e) => warn!("⚠️ Network - Node Monitoring | Failed to start node health monitoring: {:?}", e),
		};

		match self.start_node_status_monitoring(NODE_STATUS_MONITORING_LOOP_INTERVAL).await {
			Ok(_) => debug!("🔍 Network - Node Monitoring | Successfully started node status monitoring"),
			Err(e) => warn!("⚠️ Network - Node Monitoring | Failed to start node status monitoring: {:?}", e),
		};

		match self.start_ping_eligible_peers(PING_ELIGIBLE_PEERS_LOOP_INTERVAL, latest_epoch).await {
			Ok(_) => debug!("🔍 Network - Node Monitoring | Successfully started ping eligible peers"),
			Err(e) => warn!("⚠️ Network - Node Monitoring | Failed to start ping eligible peers: {:?}", e),
		};

		match self.start_node_details_status_broadcasting(NODE_DETAILS_STATUS_BROADCASTING_LOOP_INTERVAL, peer_id).await {
			Ok(_) => debug!("🔍 Network - Node Monitoring | Successfully started node details status broadcasting"),
			Err(e) => warn!("⚠️ Network - Node Monitoring | Failed to start node details status broadcasting: {:?}", e),
		};
		
		Ok(())
	}
	
    pub async fn start_node_health_monitoring(&self, loop_interval: u64) -> Result<(), Box<dyn Error + Send>> {
        let sender = self.event_sender.clone();
        info!("🏥 Network - Node Health Monitoring | Starting node health monitoring");
        let block_manager = BlockManager {};

        tokio::spawn(async move {
            let block_manager_cache = BlockManagerCache::get_instance();
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(loop_interval));
            let mut last_processed_epoch: Option<Epoch> = None;
            let mut last_aggregated_epoch: Option<Epoch> = None;

            loop {
                interval.tick().await;

                let latest_block_number = match block_manager_cache.get_last_executed_block_header().await {
                    Ok(block_header) => block_header.block_number,
                    Err(e) => {
                        error!("🚨 Network - Node Health Monitoring | Unable to get latest block number: {}", e);
                        continue;
                    }
                };

                let current_epoch = match block_manager.calculate_current_epoch(latest_block_number) {
                    Ok(epoch) => epoch,
                    Err(e) => {
                        error!("🚨 Network - Node Health Monitoring | Unable to calculate current epoch: {}", e);
                        continue;
                    }
                };

               
				debug!("🔍 Network - Node Monitoring | Block is executed, processing node health for epoch: {}, last_processed_epoch: {:?}, block_number: {}", 
							current_epoch.clone(), 
							last_processed_epoch.clone(), 
							latest_block_number.clone());

				// Process node health
				if last_processed_epoch != Some(current_epoch) {
					match block_manager.is_approaching_epoch_end(latest_block_number, 5).await {
						Ok(true) => {
							if let Err(e) = sender.send(Event::ProcessNodeHealth { epoch: current_epoch }).await
							{
								error!("🚨 Network - Node Health Monitoring | Failed to send ProcessNodeHealth event: {}", e);
							} else {
								last_processed_epoch = Some(current_epoch);
							}
						}
						Ok(false) => {}
						Err(e) => {
							error!("🚨 Network - Node Health Monitoring | Error checking if epoch is approaching end: {}", e);
						}
					}
				}

				debug!(
					"🔍 Network - Node Monitoring | Last aggregated epoch: {:?}",
					last_aggregated_epoch.clone()
				);
				// Aggregate node health
				if last_aggregated_epoch != Some(current_epoch) {
					match block_manager.is_approaching_epoch_end(latest_block_number, 10).await {
						Ok(true) => {
							if let Err(e) =
								sender.send(Event::AggregateNodeHealth { epoch: current_epoch }).await
							{
								error!("🚨 Network - Node Health Monitoring | Failed to send health update command: {}", e);
							} else {
								last_aggregated_epoch = Some(current_epoch);
							}
						}
						Ok(false) => {}
						Err(e) => {
							error!("🚨 Network - Node Health Monitoring | Error checking if epoch is approaching end: {}", e);
						}
					}
				}
                    
            }
        });

        Ok(())
    }

    pub async fn start_node_status_monitoring(&self, loop_interval: u64) -> Result<(), Box<dyn Error + Send>> {
        info!("🏥 Network - Node Status Monitoring | Node monitoring has started");
        let sender = self.event_sender.clone();
        let mut interval = tokio::time::interval(Duration::from_secs(loop_interval));
        tokio::spawn(async move {
            loop {
                interval.tick().await;
                if let Err(e) = sender.send(Event::CheckNodeStatus).await {
                    error!("🚨 Network - Node Status Monitoring | Failed to send check-node-status command: {}", e);
                }
            }
        });
        Ok(())
    }

	pub async fn start_node_details_status_broadcasting(&self, loop_interval: u64, peer_id: PeerId) -> Result<(), Box<dyn Error + Send>> {
		let sender = self.event_sender.clone();
		let mut interval = tokio::time::interval(Duration::from_secs(loop_interval));
		tokio::spawn(async move {
			loop {
				interval.tick().await;

				// Send the PublishNodeDetailedStatus event to the event sender
				if let Err(e) = sender.send(Event::PublishNodeDetailedStatus{
					peer_id: peer_id.to_string()
				}).await {
					error!("🚨 Network - Node Details Status Broadcasting | Failed to send publish-node-detailed-status command: {}", e);
				}
			}
		});
		Ok(())
	}

	// Currently not used
    // pub async fn start_peer_discovery(&self) -> Result<(), Box<dyn Error + Send>> {
    //     let sender = self.event_sender.clone();

    //     tokio::spawn(async move {
    //         let mut interval = tokio::time::interval(Duration::from_secs(10)); // Adjust interval as needed

    //         loop {
    //             interval.tick().await;

    //             // Send event to initialize/refresh peers
    //             if let Err(e) = sender.send(Event::InitializePeers).await {
    //                 error!("Failed to send InitializePeers event: {}", e);
    //             }
    //         }
    //     });

    //     Ok(())
    // }

    pub async fn start_ping_eligible_peers(&self, loop_interval: u64, current_epoch: Epoch) -> Result<(), Box<dyn Error + Send>> {
        let sender = self.event_sender.clone();
        let mut interval = tokio::time::interval(Duration::from_secs(loop_interval));
        tokio::spawn(async move {
            loop {
                interval.tick().await;

                let network_state = NetworkState::get_instance();
                let active_peers = match network_state.get_available_peers().await {
                    Ok(peers) => peers,
                    Err(e) => {
                        error!("🚨 Network - Ping Eligible Peers | Failed to get active peers: {}", e);
                        continue;
                    }
                };

                info!("🏥 Network - Ping Eligible Peers | Active peers: {:?}", active_peers);

                let _ = sender
                    .send(Event::PingEligiblePeers {
                        epoch: current_epoch, // Or determine current epoch
                        peer_ids: active_peers.iter().map(|peer| peer.to_string()).collect::<Vec<String>>(),
                    })
                    .await
                    .unwrap_or_else(|e| error!("🚨 Network - Ping Eligible Peers | Failed to send PingEligiblePeers event: {:?}", e));

            }
        });

		Ok(())
    }

}

#[async_trait]
impl NodeInfoBroadcast for Client {
    /// Command the o broadcast a node_info to the p2p network.
    async fn node_info_broadcast(&self, node_info: NodeInfo) -> Result<MessageId, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::BroadcastNodeInfo { node_info, sender })
            .await
            .unwrap_or_else(|e| {
                error!(
                    "🚨 Network - Node Info Broadcasting | Failed to send BroadcastNodeInfo command down command_sender channel: {:?}",
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
                    "🚨 Network - Transaction Broadcasting | Failed to send BroadcastTransaction command down command_sender channel: {:?}",
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
                    "🚨 Network - Node Health Broadcasting | Failed to send BroadcastTransaction command down command_sender channel: {:?}",
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
    async fn aggregated_node_health_broadcast(
        &self,
        node_health_payloads: Vec<NodeHealthPayload>,
    ) -> Result<MessageId, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::BroadcastNodeHealthPayload {
                node_health_payloads,
                sender,
            })
            .await
            .unwrap_or_else(|e| {
                error!(
                    "🚨 Network - Node Health Broadcasting | Failed to send BroadcastTransaction command down command_sender channel: {:?}",
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
    async fn block_validate_broadcast(&self, block_payload: BlockPayload) -> Result<MessageId, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::BroadcastValidateBlock { block_payload, sender })
            .await
            .unwrap_or_else(|e| {
                error!("🚨 Network - Block Broadcasting | Failed to send BroadcastBlock command down command_sender channel: {e:?}");
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
            .send(Command::BroadcastBlockProposer {
                block_proposer_payload,
                sender,
            })
            .await
            .unwrap_or_else(|e| {
                error!("🚨 Network - Block Proposer Broadcasting | Failed to send BroadcastBlockProposer command down command_sender channel: {e:?}");
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
                error!("🚨 Network - Vote Broadcasting | Failed to send BroadcastVote command down command_sender channel: {e:?}");
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
    async fn vote_result_broadcast(&self, vote_result: VoteResult) -> Result<MessageId, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::BroadcastVoteResult { vote_result, sender })
            .await
            .unwrap_or_else(|e| {
                error!("🚨 Network - Vote Result Broadcasting | Failed to send BroadcastVoteResult command down command_sender channel: {e:?}");
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
            .send(Command::QueryBlockRequest {
                peer_id,
                request: L1xRequest::QueryBlock(request),
                sender,
            })
            .await
            .unwrap_or_else(|e| {
                error!("🚨 Network - Block Query Request | Failed to query for block command down command_sender channel: {e:?}");
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
        channel: ResponseChannel<L1xResponse>,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::QueryBlockResponse {
                channel,
                response,
                sender,
            })
            .await
            .unwrap_or_else(|e| {
                error!("🚨 Network - Block Query Response | Failed to send response for query block command down command_sender channel: {e:?}");
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
            .send(Command::QueryStatusRequest {
                peer_id,
                request: L1xRequest::QueryNodeStatus(request_time),
                sender,
            })
            .await
            .unwrap_or_else(|e| {
                error!("🚨 Network - Query Status Request | Failed to query for status command down command_sender channel: {e:?}");
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
impl BroadcastNodeDetailedStatus for Client {
	async fn broadcast_node_detailed_status(
		&self,
		node_detailed_status: NodeDetailedStatus,
	) -> Result<MessageId, Box<dyn Error + Send>> {
		let (sender, receiver) = oneshot::channel();
		self.sender
			.send(Command::BroadcastNodeDetailedStatus {
				node_detailed_status,
				sender,
			})
			.await
			.unwrap_or_else(|e| {
				error!("🚨 Network - Node Details Status Broadcasting | Failed to send node detailed status command down command_sender channel: {e:?}");
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

pub struct EventLoop {
    swarm: Swarm<Behaviour>,
    command_receiver: mpsc::Receiver<Command>,
    event_sender: mpsc::Sender<Event>,
    pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>,
    network_state: &'static Arc<NetworkState>,
    eth_chain_id: Option<u64>,
    cluster_address: Option<String>,
}

impl EventLoop {
    fn new(
        swarm: Swarm<Behaviour>,
        command_receiver: mpsc::Receiver<Command>,
        event_sender: mpsc::Sender<Event>,
        dht_health_storage: DHTHealthStorage,
        eth_chain_id: Option<u64>,
        cluster_address: Option<String>,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_dial: Default::default(),
            network_state: NetworkState::get_instance(),
            eth_chain_id,
            cluster_address,
        }
    }

    /// Start the event loop. This will listen for commands from the node (itself) and events from
    /// p2p network,
    pub async fn run(mut self) {
        let local_peer_id = *self.swarm.local_peer_id();
        let mut refresh_interval = tokio::time::interval(Duration::from_secs(300));

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
                        warn!("⚠️ Network - Event Loop | event_loop worker exited");
                        return
                    },
                },
                _ = refresh_interval.tick() => {
                    debug!("🔍 Network - Event Loop | Refreshing swarm");
                    self.swarm.behaviour_mut().kademlia.get_closest_peers(local_peer_id);
                },
            }
        }
    }

    /// Handles events received from the p2p network. This can result in anything from logging some
    /// info to sending a transaction for mempool validation.
    async fn handle_event(
        &mut self,
        event: SwarmEvent<
            BehaviourEvent,
            either::Either<
                either::Either<
                    either::Either<either::Either<std::io::Error, std::io::Error>, Void>,
                    ConnectionHandlerUpgrErr<std::io::Error>,
                >,
                ConnectionHandlerUpgrErr<std::io::Error>,
            >,
        >,
    ) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                let local_peer_id = *self.swarm.local_peer_id();
                info!(
                    "🏥 Network - New Listen Address | Local node is listening on {:?}",
                    address.with(Protocol::P2p(local_peer_id.into()))
                );
            }
            SwarmEvent::IncomingConnection { .. } => {}
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                if endpoint.is_dialer() {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Ok(()));
                    }

                    info!(
                        "🏥 Network - Connection Established | Swarm Event | Peer ID: {:?}, Remote Address: {:?}",
                        peer_id,
                        endpoint.get_remote_address()
                    );
                    

                    // Update the active peers to lazy static ACTIVE_PEERS
                    let network_state = NetworkState::get_instance();
                    match network_state.add_active_peer(peer_id).await {
                        Ok(_) => debug!("🔍 Network - Connection Established | Swarm Event | Peer ID: {:?} added to ACTIVE_PEERS", peer_id),
                        Err(e) => error!(
                            "🚨 Network - Connection Established | Swarm Event | Peer ID: {:?} failed to add to ACTIVE_PEERS: {:?}",
                            peer_id, e
                        ),
                    }
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                cause,
                endpoint,
                ..
            } => {
                if let Some(cause) = cause {
                    warn!("⚠️ Network - Connection Closed | Connection closed with peer {}: {}", peer_id, cause);
                } else {
                    warn!("⚠️ Network - Connection Closed | Connection closed with peer {}", peer_id);
                }
                // self.handle_peer_disconnection(peer_id).await;

                debug!("🔍 Network - Connection Closed | Swarm Event | Peer ID: {:?}", peer_id);

                let network_state = NetworkState::get_instance();
                match network_state.remove_active_peer(peer_id).await {
                    Ok(_) => debug!("🔍 Network - Connection Closed | Swarm Event | Peer ID: {:?} removed from ACTIVE_PEERS", peer_id),
                    Err(e) => error!(
                        "🚨 Network - Connection Closed | Swarm Event | Peer ID: {:?} failed to remove from ACTIVE_PEERS: {:?}",
                        peer_id, e
                    ),
                }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(peer_id) = peer_id {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Err(Box::new(error)));
                    }

                    // Remove the peer from the active peers list
                    let network_state = NetworkState::get_instance();
                    let _ = network_state.remove_active_peer(peer_id).await;
                }
            }
            SwarmEvent::IncomingConnectionError {
                error,
                local_addr,
                send_back_addr,
            } => {
                warn!("⚠️ Network - Incoming Connection Error | Error: local_addr={local_addr:?}, send_back_addr={send_back_addr:?}, error={error:?}")
            }
            SwarmEvent::Dialing(peer_id) => info!("🏥 Network - Dialing | Dialing Peer ID: {peer_id}"),
            SwarmEvent::Behaviour(event) => match event {
                BehaviourEvent::Identify(event) => match event {
                    // Prints peer id identify info is being sent to.
                    identify::Event::Sent { peer_id, .. } => {
                        info!("🏥 Network - Identify | Sent identify info to Peer ID:{peer_id:?}")
                    }
                    // Prints out the info received via the identify event
                    identify::Event::Received { peer_id, info } => {
                        info!("🏥 Network - Identify | Received {info:?} from Peer ID: {peer_id:?}");
                        let local_peer_id = self.swarm.local_peer_id().clone();
                        self.swarm.behaviour_mut().kademlia.get_closest_peers(local_peer_id);
                    }
                    _ => info!("🏥 Network - Identify | Some Identify event received: {event:?}"),
                },
                BehaviourEvent::Kademlia(event) => match event {
                    KademliaEvent::OutboundQueryProgressed { result, id, .. } => match result {
                        QueryResult::Bootstrap(result) => match result {
                            Ok(res) => {
                                info!("🏥 Network - Kademlia | Bootstrap Success: {res:?}");
                                // Initialize peers after successful bootstrap
                                let mut peer_ids: Vec<String> = Vec::new();
                                for bucket in self.swarm.behaviour_mut().kademlia.kbuckets() {
                                    for entry in bucket.iter() {
                                        let peer_id = entry.node.key.clone().into_preimage();
                                        peer_ids.push(peer_id.to_string());
                                    }
                                }

                                if !peer_ids.is_empty() {
                                    let _ = self
                                        .event_sender
                                        .send(Event::PingEligiblePeers {
                                            epoch: 0, // Or determine current epoch
                                            peer_ids,
                                        })
                                        .await
                                        .unwrap_or_else(|e| error!("🚨 Network - Ping Eligible Peers | Failed to send PingEligiblePeers event: {:?}", e));
                                }
                            }
                            Err(e) => {
                                error!("🚨 Network - Kademlia | Bootstrap Failure: {e:?}");
                            }
                        },
                        QueryResult::GetClosestPeers(result) => match result {
                            Ok(res) => {
                                let mut futures = FuturesUnordered::new();
                                let closest_peers = res.peers.clone();

                                for peer_id in res.peers.iter().take(MAX_CLOSEST_PEERS) {
                                    // Connect to up to 5 peers
                                    let addrs = self
                                        .swarm
                                        .behaviour_mut()
                                        .kademlia
                                        .addresses_of_peer(peer_id)
                                        .into_iter()
                                        .filter(is_public_address)
                                        .collect::<Vec<_>>();
                                    for addr in addrs {
                                        let (oneshot_tx, oneshot_rx) = oneshot::channel();
                                        let dial_cmd = Command::Dial {
                                            peer_id: *peer_id,
                                            peer_addr: addr.clone(),
                                            sender: oneshot_tx,
                                        };
                                        self.handle_command(dial_cmd).await;
                                        futures.push(async move { (*peer_id, oneshot_rx.await) });
                                    }
                                }

                                while let Some((peer_id, result)) = futures.next().await {
                                    match result {
                                        Ok(Ok(())) => info!("🏥 Network - Kademlia | Successfully connected to a peer: {peer_id:}"),
                                        Ok(Err(e)) => {
                                            debug!("🔍 Network - Kademlia | Failed to connect to a peer: {peer_id:}, error: {e:}");
                                            // Remove the peer from Kademlia and local store
                                            self.swarm.behaviour_mut().kademlia.remove_peer(&peer_id);
                                        }
                                        Err(e) => debug!("🔍 Network - Kademlia | Failed to receive result for peer: {peer_id:}, error: {e:}"),
                                    }
                                }

                                // Update the active peers to lazy static NetworkState
								let active_peers: Vec<PeerId> = closest_peers.iter().map(|peer_id| *peer_id).collect();

								// Add the active peers to the lazy static NetworkState
                                if !active_peers.is_empty() {

									// Add the active peers to the lazy static NetworkState
									{
										let network_state = NetworkState::get_instance();
										let _ = network_state
										.add_active_peers(active_peers.clone())
										.await;
									}

									// Get Latest Epoch from the last executed block header
									let current_epoch = {
										let block_manager_cache = BlockManagerCache::get_instance();
										match block_manager_cache.get_last_executed_block_header().await {
											Ok(last_executed_block_header) => last_executed_block_header.epoch,
											Err(e) => {
												error!("🚨 Network - Kademlia | Failed to get last executed block header: {:?}", e);
												0
											}
										}
									};

									// Send the PingEligiblePeers event to the event sender
                                    let _ = self
                                        .event_sender
                                        .send(Event::PingEligiblePeers {
                                            epoch: current_epoch,
                                            peer_ids: active_peers.iter().map(|peer_id| peer_id.to_string()).collect(),
                                        })
                                        .await
                                        .unwrap_or_else(|e| error!("🚨 Network - Ping Eligible Peers | Failed to send PingEligiblePeers event: {:?}", e));
                                }
                            }
                            Err(e) => {
                                warn!("⚠️ Network - Kademlia | GetClosestPeers query failed: {:?}", e);
                            }
                        },
                        _ => info!("🏥 Network - Kademlia | Some OutboundQueryProgressed event received: {result:?}"),
                    },
                    KademliaEvent::RoutingUpdated { peer, .. } => {
                        info!("🏥 Network - Kademlia | Kademlia routing updated: {peer:?}")
                    }
                    _ => info!("🏥 Network - Kademlia | Some Kademlia event received: {event:?}"),
                },
                BehaviourEvent::Gossipsub(event) => match event {
                    gossipsub::Event::Subscribed { peer_id, topic } => {
                        info!("🏥 Network - Gossipsub | Peer ID: {peer_id:?} subscribed to topic: '{topic:?}'");

                        // Access addresses through the Kademlia routing table
                        let addresses: Vec<Multiaddr> = self
                            .swarm
                            .behaviour_mut()
                            .kademlia
                            .kbuckets()
                            .flat_map(|bucket| {
                                bucket
                                    .iter()
                                    .filter_map(|entry| {
                                        if *entry.node.key.preimage() == peer_id {
                                            Some(
                                                entry
                                                    .node
                                                    .value
                                                    .iter()
                                                    .cloned()
                                                    .map(|addr| addr.with(Protocol::P2p(peer_id.into())))
                                                    .collect::<Vec<_>>(),
                                            )
                                        } else {
                                            None
                                        }
                                    })
                                    .flatten()
                                    .collect::<Vec<_>>()
                            })
                            .collect();

                        if addresses.is_empty() {
                            debug!("🔍 Network - Gossipsub | No known addresses for peer: {peer_id:?}");
                        } else {
                            for addr in &addresses {
                                info!("🏥 Network - Gossipsub | Peer address: {addr} for peer: {peer_id:?}");
                            }
                        }

                        let network_state = NetworkState::get_instance();
                        match network_state.add_active_peer(peer_id).await {
                            Ok(_) => debug!("🔍 Network - Gossipsub | Peer: {peer_id:?} added to ACTIVE_PEERS"),
                            Err(e) => error!("🚨 Network - Gossipsub | Peer: {peer_id:?} failed to add to ACTIVE_PEERS: {:?}", e),
                        }
                    }
                    gossipsub::Event::Unsubscribed { peer_id, topic } => {
                        info!("🏥 Network - Gossipsub | Peer ID: {peer_id:?} unsubscribed from topic: '{topic:?}'");

                        // let network_state = NetworkState::get_instance();
                        // match network_state.remove_active_peer(peer_id).await {
                        // 	Ok(_) => debug!("Peer: {peer_id:?} removed from ACTIVE_PEERS"),
                        // 	Err(e) => error!("Peer: {peer_id:?} failed to remove from ACTIVE_PEERS: {:?}", e),
                        // }
                    }
                    gossipsub::Event::Message {
                        propagation_source: peer_id,
                        message_id: id,
                        message,
                    } => {
                        // Are we recieving a transaction/block_payload/etc
                        match message.topic.as_str() {
                            NODE_INFO_TOPIC => match deserialize_from_versioned_message::<NodeInfo>(&message.data) {
                                Ok(node_info) => {

									let network_state = NetworkState::get_instance();
									// Add the active peer to the lazy static NetworkState
									{
										match network_state.add_active_peer(peer_id).await {
											Ok(_) => debug!("🔍 Network - Gossipsub | Peer: {peer_id:?} added to ACTIVE_PEERS"),
											Err(e) => error!("🚨 Network - Gossipsub | Peer: {peer_id:?} failed to add to ACTIVE_PEERS: {:?}", e),
										}
									}

									// Update the active peer address in the lazy static NetworkState
									{
										let address = node_info.address.clone();
										match network_state.update_active_peer_address(peer_id, address).await {
											Ok(_) => debug!("🔍 Network - Gossipsub | Peer: {peer_id:?} updated with address: {address:?}"),
											Err(e) => error!("🚨 Network - Gossipsub | Peer: {peer_id:?} failed to update with address: {address:?}: {:?}", e),
										}
									}

									
                                    let _ = self
                                        .event_sender
                                        .send(Event::InboundNodeInfo { node_info })
                                        .await
                                        .unwrap_or_else(|e| {
                                            error!("🚨 Network - Gossipsub | Failed to send incoming node_info to receiver: {:?}", e)
                                        });
                                }
                                Err(e) => {
                                    warn!("⚠️ Network - Gossipsub | Can't deserialize NodeInfo from peer {}: {}", peer_id, e)
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
                                                error!("🚨 Network - Gossipsub | Failed to send incoming tx to receiver: {:?}", e)
                                            });
                                    }
                                    Err(e) => {
                                        warn!("⚠️ Network - Gossipsub | Can't deserialize Transaction from peer {}: {}", peer_id, e)
                                    }
                                }
                            }
                            BLOCKS_VALIDATE_TOPIC => {
                                match deserialize_from_versioned_message::<BlockPayload>(&message.data) {
                                    Ok(block_payload) => {
                                        // Initialize eligible peers for ping results
                                        if block_payload.block.block_header.block_number
                                            == ELIGIBLE_PEERS_INIT_BLOCK_NUMBER
                                        {
                                            let mut peer_ids: Vec<String> = Vec::new();
                                            for bucket in self.swarm.behaviour_mut().kademlia.kbuckets() {
                                                for entry in bucket.iter() {
                                                    let peer_id = entry.node.key.clone().into_preimage(); // Extract the PeerId
                                                    peer_ids.push(peer_id.to_string());
                                                }
                                            }
                                            let _ = self
                                                .event_sender
                                                .send(Event::PingEligiblePeers { epoch: 0, peer_ids })
                                                .await
                                                .unwrap_or_else(|e| {
                                                    error!("🚨 Network - Gossipsub | Failed to send PingEligiblePeers event: {:?}", e)
                                                });
                                        }

                                        let _ = self
                                            .event_sender
                                            .send(Event::InboundValidateBlock { block_payload })
                                            .await
                                            .unwrap_or_else(|e| {
                                                error!(
                                                    "🚨 Network - Gossipsub | Failed to send incoming block_payload validate to receiver: {:?}",
                                                    e
                                                )
                                            });
                                    }
                                    Err(e) => {
                                        warn!("⚠️ Network - Gossipsub | Can't deserialize BlockValidatePayload from peer {}: {}", peer_id, e)
                                    }
                                }
                            }
                            BLOCK_PROPOSER_TOPIC => {
                                match deserialize_from_versioned_message::<BlockProposerPayload>(&message.data) {
                                    Ok(block_proposer_payload) => {
                                        let _ = self
                                            .event_sender
                                            .send(Event::InboundBlockProposer { block_proposer_payload })
                                            .await
                                            .unwrap_or_else(|e| {
                                                error!(
                                                    "🚨 Network - Gossipsub | Failed to send incoming block_proposer_payload to receiver: {:?}",
                                                    e
                                                )
                                            });
                                    }
                                    Err(e) => {
                                        warn!("⚠️ Network - Gossipsub | Can't deserialize BlockProposerPayload from peer {}: {}", peer_id, e)
                                    }
                                }
                            }
                            VOTE_TOPIC => match deserialize_from_versioned_message::<Vote>(&message.data) {
                                Ok(vote) => {
                                    let _ =
                                        self.event_sender.send(Event::InboundVote { vote }).await.unwrap_or_else(|e| {
                                            error!("🚨 Network - Gossipsub | Failed to send incoming vote to receiver: {:?}", e)
                                        });
                                }
                                Err(e) => {
                                    warn!("⚠️ Network - Gossipsub | Can't deserialize Vote: {}", e)
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
                                                error!("🚨 Network - Gossipsub | Failed to send incoming vote_result to receiver: {:?}", e)
                                            });
                                    }
                                    Err(e) => {
                                        warn!("⚠️ Network - Gossipsub | Can't deserialize VoteResult from peer {}: {}", peer_id, e)
                                    }
                                }
                            }
                            NODE_HEALTH_TOPIC => {
                                match deserialize_from_versioned_message::<Vec<NodeHealth>>(&message.data) {
                                    Ok(node_healths) => {
                                        let _ = self
                                            .event_sender
                                            .send(Event::InboundNodeHealth { node_healths })
                                            .await
                                            .unwrap_or_else(|e| {
                                                error!("🚨 Network - Gossipsub | Failed to send incoming node_health to receiver: {:?}", e)
                                            });
                                    }
                                    Err(e) => {
                                        warn!("⚠️ Network - Gossipsub | Can't deserialize NodeHealth from peer {}: {}", peer_id, e)
                                    }
                                }
                            }
                            AGGREGATED_NODE_HEALTH_TOPIC => {
                                match deserialize_from_versioned_message::<Vec<NodeHealthPayload>>(&message.data) {
                                    Ok(aggregated_healths) => {
                                        let _ = self
                                            .event_sender
                                            .send(Event::InboundAggregatedNodeHealth { aggregated_healths })
                                            .await
                                            .unwrap_or_else(|e| {
                                                error!(
                                                    "🚨 Network - Gossipsub | Failed to send incoming aggregated_node_health to receiver: {:?}",
                                                    e
                                                )
                                            });
                                    }
                                    Err(e) => {
                                        warn!("⚠️ Network - Gossipsub | Can't deserialize Aggregated NodeHealth from peer {}: {}", peer_id, e)
                                    }
                                }
                            }
                            BROADCAST_NODE_DETAILED_STATUS_TOPIC => {
                                match deserialize_from_versioned_message::<NodeDetailedStatus>(&message.data) {
								
                                    Ok(node_detailed_status) => {
										debug!("🔍 Network - Gossipsub | Received node detailed status from peer: {:?}, node_detailed_status: {:?}", peer_id, node_detailed_status.clone());
                                        let _ = self
                                            .event_sender
                                            .send(Event::InboundNodeDetailedStatus(node_detailed_status))
                                            .await
                                            .unwrap_or_else(|e| {
                                                error!("🚨 Network - Gossipsub | Failed to send incoming node detailed status to receiver: {:?}", e)
                                            });
                                    }
									Err(e) => {
										warn!("⚠️ Network - Gossipsub | Can't deserialize NodeDetailedStatus from peer {}: {}", peer_id, e)
									}
								}
                            }
                            _ => {
                                warn!(
                                    "⚠️ Network - Gossipsub | Topic {} not supported. Shouldn't recieve an unknown or un-subscribed from topic.",
                                    message.topic.as_str()
                                );
                            }
                        }
                        debug!("🔍 Network - Gossipsub | New p2p message received with id: {id} from peer: {peer_id}")
                    }
                    _ => info!("🏥 Network - Gossipsub | Some Gossipsub event received: {event:?}"),
                },
                BehaviourEvent::AutoNat(event) => match event {
                    autonat::Event::InboundProbe(inbound_event) => match inbound_event {
                        autonat::InboundProbeEvent::Error { peer, error, .. } => {
                            debug!("🔍 Network - AutoNAT | Inbound Probe failed with Peer: {}. Error: {:#?}.", peer, error);
                        }
                        _ => {
                            log::trace!("🔍 Network - AutoNAT | Inbound Probe: {:#?}", inbound_event);
                        }
                    },
                    autonat::Event::OutboundProbe(outbound_event) => match outbound_event {
                        autonat::OutboundProbeEvent::Error { peer, error, .. } => {
                            debug!(
                                "🔍 Network - AutoNAT | Outbound Probe failed with Peer: {:#?}. Error: {:#?}",
                                peer, error
                            );
                        }
                        _ => {
                            log::trace!("🔍 Network - AutoNAT | Outbound Probe: {:#?}", outbound_event);
                        }
                    },
                    autonat::Event::StatusChanged { old, new } => {
                        debug!("🔍 Network - AutoNAT | Old status: {:#?}. AutoNAT New status: {:#?}", old, new);
                        let local_peer_id = self.swarm.local_peer_id().clone();
                        let behaviour = self.swarm.behaviour_mut();
                        match new {
                            autonat::NatStatus::Public(addr) => {
                                // Log the discovery of a new public address
                                info!("🏥 Network - AutoNAT | Discovered public address: {}", addr);
                                behaviour.kademlia.add_address(&local_peer_id, addr.clone());
                                // Share public address with other nodes
                                behaviour.kademlia.get_closest_peers(local_peer_id);
                                info!("🏥 Network - AutoNAT | Added public address {} for peer {}", addr, local_peer_id);
                            }
                            autonat::NatStatus::Private => {
                                if let Some(addr) = match old {
                                    autonat::NatStatus::Public(addr) => Some(addr),
                                    _ => None,
                                } {
                                    warn!("⚠️ Network - AutoNAT | Peer changed to private or unknown address");
                                    // Remove peer from the routing table and address_peers
                                    behaviour.kademlia.remove_address(&local_peer_id, &addr);
                                    info!("🏥 Network - AutoNAT | Removed address {} for peer {}", addr, local_peer_id);
                                }
                            }
                            autonat::NatStatus::Unknown => {
                                info!("🏥 Network - AutoNAT | Peer address is unknown")
                            }
                        }
                    }
                },
                BehaviourEvent::RequestResponse(event) => match event {
                    request_response::Event::Message {
                        peer: sender_peer,
                        message,
                    } => match message {
                        request_response::Message::Request { request, channel, .. } => {
                            debug!("🔍 Network - Request Response | Received request: {:?} from channel: {:?}", request, channel);
                            match request {
                                L1xRequest::QueryBlock(request) => self
                                    .event_sender
                                    .send(Event::InboundQueryBlockRequest { request, channel })
                                    .await
                                    .unwrap_or_else(|e| {
                                        error!("🚨 Network - Request Response | Failed to send incoming query block request to receiver: {:?}", e)
                                    }),
                                L1xRequest::QueryNodeStatus(request_time) => {
                                    let local_peer_id = self.swarm.local_peer_id().clone();
                                    let response =
                                        L1xResponse::QueryNodeStatus(local_peer_id.to_string(), request_time);
                                    self.swarm
                                        .behaviour_mut()
                                        .request_response
                                        .send_response(channel, response)
                                        .unwrap_or_else(|e| {
                                            error!("🚨 Network - Request Response | Failed to send query status response to receiver: {:?}", e)
                                        });
                                }
                            }
                        }
                        request_response::Message::Response { response, .. } => {
                            debug!("🔍 Network - Request Response | Received response: {:?}", response);
                            match response {
                                L1xResponse::QueryBlock {
                                    block_payload,
                                    is_finalized,
                                    vote_result,
                                } => self
                                    .event_sender
                                    .send(Event::InboundQueryBlockResponse {
                                        block_payload,
                                        is_finalized,
                                        vote_result,
                                    })
                                    .await
                                    .unwrap_or_else(|e| {
                                        error!("🚨 Network - Request Response | Failed to send incoming query block response to receiver: {:?}", e)
                                    }),
                                L1xResponse::QueryBlockError(error_message) => {
                                    warn!("⚠️ Network - Request Response | Query block error: {:?}", error_message);
                                }

                                L1xResponse::QueryNodeStatus(_, request_time) => {
                                    let peer_id = sender_peer.to_string(); // Use the actual sender peer id
                                    debug!(
                                        "🔍 Network - Request Response | Received L1xResponse::QueryNodeStatus response from peer: {:?}",
                                        peer_id
                                    );
                                    if let Ok(current_timestamp) = util::generic::current_timestamp_in_millis() {
                                        let response_time = current_timestamp - request_time;
                                        debug!("🔍 Network - Request Response | Sending Event::PingResult event to event_sender channel, peer_id: {:?}, is_success: {:?}, rtt: {:?}", peer_id, true, response_time as u64);
                                        self.event_sender
                                            .send(Event::PingResult {
                                                peer_id,
                                                is_success: true,
                                                rtt: response_time as u64,
                                            })
                                            .await
                                            .unwrap_or_else(|e| {
                                                error!(
                                                    "🚨 Network - Request Response | Failed to send incoming query status response to receiver: {:?}",
                                                    e
                                                )
                                            })
                                    } else {
                                        error!("🚨 Network - Request Response | Failed to get current timestamp in L1xResponse")
                                    }
                                } // Add other response types

								L1xResponse::QueryNodeDetailedStatus(node_detailed_status) => {
									let peer_id = sender_peer.to_string(); // Use the actual sender peer id
									debug!(
										"🔍 Network - Request Response | Received L1xResponse::QueryNodeDetailedStatus response from peer: {:?}",
										peer_id
									);
									self.event_sender
										.send(Event::InboundNodeDetailedStatus(node_detailed_status))
										.await
										.unwrap_or_else(|e| {
											error!("🚨 Network - Request Response | Failed to send incoming node detailed status to receiver: {:?}", e)
										});
								}
                            }
                        }
                    },
                    _ => {}
                },
            },
            e => warn!("⚠️ Network - Behaviour | Behaviour: {e:?}"),
        }
    }

    /// Given a vallid `Command`, execute the proper underlying libp2p calls
    async fn handle_command(&mut self, command: Command) {
        debug!("🔍 Network - Command | Handling command: {:?}", command);
        match command {
            Command::StartListening { addr, sender } => {
                let _ = match self.swarm.listen_on(addr) {
                    Ok(_) => sender.send(Ok(())),
                    Err(e) => sender.send(Err(Box::new(e))),
                };
            }
            Command::Dial {
                peer_id,
                peer_addr,
                sender,
            } => {
                if let hash_map::Entry::Vacant(e) = self.pending_dial.entry(peer_id) {
                    match self.swarm.dial(peer_addr.clone().with(Protocol::P2p(peer_id.into()))) {
                        Ok(()) => {
                            e.insert(sender);

                            self.swarm
                                .behaviour_mut()
                                .kademlia
                                // .add_address(&peer_id, "/dnsaddr/bootstrap.libp2p.io".parse()?);
                                .add_address(&peer_id, peer_addr);
                        }
                        Err(e) => {
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    }
                } else {
                    todo!("Already dialing peer.");
                }
            }
            Command::BroadcastNodeInfo { node_info, sender } => match serialize_as_versioned_message(node_info) {
                Ok(tx_bytes) => match self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(gossipsub::IdentTopic::new(NODE_INFO_TOPIC), tx_bytes)
                {
                    Ok(msg_id) => {
                        info!("🏥 Network - Gossipsub | Broadcast Node Info");
                        let _ = sender.send(Ok(msg_id));
                    }
                    Err(e) => {
                        // error!("FAILED TO PUBLISH NodeInfo MESSAGE ");
                        let _ = sender.send(Err(Box::new(e)));
                    }
                },
                Err(e) => {
                    error!("🚨 Network - Gossipsub | Failed to serialize NodeInfo to bytes");
                    let _ = sender.send(Err(e.into()));
                }
            },
            Command::BroadcastTransaction { transaction, sender } => {
                match serialize_as_versioned_message(transaction) {
                    Ok(tx_bytes) => match self
                        .swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new(TRANSACTIONS_TOPIC), tx_bytes)
                    {
                        Ok(msg_id) => {
                            info!("🏥 Network - Gossipsub | Broadcast Transaction");
                            let _ = sender.send(Ok(msg_id));
                        }
                        Err(e) => {
                            // error!("FAILED TO PUBLISH MESSAGE");
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    },
                    Err(e) => {
                        error!("🚨 Network - Gossipsub | Failed to serialize Transaction to bytes");
                        let _ = sender.send(Err(e.into()));
                    }
                }
            }
            Command::BroadcastValidateBlock { block_payload, sender } => {
                match serialize_as_versioned_message(block_payload) {
                    Ok(block_bytes) => match self
                        .swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new(BLOCKS_VALIDATE_TOPIC), block_bytes)
                    {
                        Ok(msg_id) => {
                            info!("🏥 Network - Gossipsub | Broadcast Validate Block");
                            let _ = sender.send(Ok(msg_id));
                        }
                        Err(e) => {
                            // error!("FAILED TO PUBLISH BLOCK");
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    },
                    Err(e) => {
                        error!("🚨 Network - Gossipsub | Failed to serialize Block to bytes");
                        let _ = sender.send(Err(e.into()));
                    }
                }
            }
            Command::BroadcastBlockProposer {
                block_proposer_payload,
                sender,
            } => match serialize_as_versioned_message(block_proposer_payload) {
                Ok(cluster_block_proposers_bytes) => {
                    match self.swarm.behaviour_mut().gossipsub.publish(
                        gossipsub::IdentTopic::new(BLOCK_PROPOSER_TOPIC),
                        cluster_block_proposers_bytes,
                    ) {
                        Ok(msg_id) => {
                            info!("🏥 Network - Gossipsub | Broadcast Block Proposer");
                            let _ = sender.send(Ok(msg_id));
                        }
                        Err(e) => {
                            // error!("FAILED TO PUBLISH BLOCK PROPOSER");
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    }
                }
                Err(e) => {
                    error!("🚨 Network - Gossipsub | Failed to serialize Block Proposer to bytes");
                    let _ = sender.send(Err(e.into()));
                }
            },
            Command::BroadcastVote { vote, sender } => match serialize_as_versioned_message(vote.clone()) {
                Ok(cluster_vote_bytes) => {
                    match self
                        .swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new(VOTE_TOPIC), cluster_vote_bytes)
                    {
                        Ok(msg_id) => {
                            info!("🏥 Network - Gossipsub | Broadcasting Vote for Block: {}", vote.clone().data.block_number);
                            let _ = sender.send(Ok(msg_id));
                        }
                        Err(e) => {
                            // error!("FAILED TO PUBLISH VOTE");
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    }
                }
                Err(e) => {
                    error!("🚨 Network - Gossipsub | Failed to serialize Vote to bytes");
                    let _ = sender.send(Err(e.into()));
                }
            },
            Command::BroadcastVoteResult { vote_result, sender } => match serialize_as_versioned_message(vote_result) {
                Ok(cluster_vote_result_bytes) => {
                    match self
                        .swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new(VOTE_RESULT_TOPIC), cluster_vote_result_bytes)
                    {
                        Ok(msg_id) => {
                            info!("🏥 Network - Gossipsub | Broadcast Vote Result");
                            let _ = sender.send(Ok(msg_id));
                        }
                        Err(e) => {
                            // error!("FAILED TO PUBLISH VOTE RESULT");
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    }
                }
                Err(e) => {
                    error!("🚨 Network - Gossipsub | Failed to serialize Vote Result to bytes");
                    let _ = sender.send(Err(e.into()));
                }
            },
            Command::BroadcastNodeHealth { node_healths, sender } => {
                match serialize_as_versioned_message(node_healths.clone()) {
                    Ok(healths) => {
                        match self
                            .swarm
                            .behaviour_mut()
                            .gossipsub
                            .publish(gossipsub::IdentTopic::new(NODE_HEALTH_TOPIC), healths)
                        {
                            Ok(msg_id) => {
                                info!("🏥 Network - Gossipsub | Broadcast Node Health");
                                let _ = sender.send(Ok(msg_id));
                            }
                            Err(e) => {
                                // error!("FAILED TO PUBLISH NODE HEALTH");
                                let _ = sender.send(Err(Box::new(e)));
                            }
                        }
                    }
                    Err(e) => {
                        error!("🚨 Network - Gossipsub | Failed to serialize Node Health to bytes");
                        let _ = sender.send(Err(e.into()));
                    }
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
            }
            Command::BroadcastNodeHealthPayload {
                node_health_payloads,
                sender,
            } => match serialize_as_versioned_message(node_health_payloads) {
                Ok(health) => {
                    match self
                        .swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new(AGGREGATED_NODE_HEALTH_TOPIC), health)
                    {
                        Ok(msg_id) => {
                            info!("🏥 Network - Gossipsub | Broadcast Node Health Payload");
                            let _ = sender.send(Ok(msg_id));
                        }
                        Err(e) => {
                            // error!("FAILED TO PUBLISH NODE HEALTH PAYLOAD");
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    }
                }
                Err(e) => {
                    error!("🚨 Network - Gossipsub | Failed to serialize Node Health Payload to bytes");
                    let _ = sender.send(Err(e.into()));
                }
            },
            Command::TopicSubscribe { topic_string, sender } => {
                let topic = gossipsub::IdentTopic::new(topic_string.clone());
                match self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    Ok(_) => {
                        let _ = sender.send(Ok(()));
                    }
                    Err(e) => {
                        error!("🚨 Network - Gossipsub | Failed to subscribe to topic {topic_string:?}: {e:?}");
                        let _ = sender.send(Err(Box::new(e)));
                    }
                }
            }
            Command::TopicUnsubscribe { topic_string, sender } => {
                let topic = gossipsub::IdentTopic::new(topic_string.clone());
                match self.swarm.behaviour_mut().gossipsub.unsubscribe(&topic) {
                    Ok(_) => {
                        let _ = sender.send(Ok(()));
                    }
                    Err(e) => {
                        error!("🚨 Network - Gossipsub | Failed to unsubscribe from topic {topic_string:?}: {e:?}");
                        let _ = sender.send(Err(Box::new(e)));
                    }
                }
            }
            Command::QueryBlockRequest {
                peer_id,
                request,
                sender,
            } => {
                let request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, request);
                let _ = sender.send(Ok(request_id));
            }
            Command::QueryBlockResponse {
                channel,
                response,
                sender,
            } => match self.swarm.behaviour_mut().request_response.send_response(channel, response) {
                Ok(_) => {
                    let _ = sender.send(Ok(()));
                }
                Err(e) => {
                    error!("🚨 Network - Request Response | Failed to send response for request-response protocol{:?}", e);
                    let _ = sender.send(Err(Box::new(io::Error::new(
                        io::ErrorKind::Other,
                        "Send response error",
                    ))));
                }
            },
            Command::QueryStatusRequest {
                peer_id,
                request,
                sender,
            } => {
                let request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, request);
                let _ = sender.send(Ok(request_id));
            },
			Command::BroadcastNodeDetailedStatus {
				node_detailed_status,
				sender,
			} => {
				debug!("🔍 Network - Command | Broadcasting node detailed status to network: {:?}", node_detailed_status);
				match serialize_as_versioned_message(node_detailed_status.clone()) {
                    Ok(node_detailed_status_bytes) => {
                        match self
                            .swarm
                            .behaviour_mut()
                            .gossipsub
                            .publish(gossipsub::IdentTopic::new(BROADCAST_NODE_DETAILED_STATUS_TOPIC), node_detailed_status_bytes)
                        {
                            Ok(msg_id) => {
                                info!("🏥 Network - Gossipsub | Broadcast Node Detailed Status");
                                let _ = sender.send(Ok(msg_id));
                            }
                            Err(e) => {
                                // error!("FAILED TO PUBLISH NODE HEALTH");
                                let _ = sender.send(Err(Box::new(e)));
                            }
                        }
                    }
                    Err(e) => {
                        error!("🚨 Network - Gossipsub | Failed to serialize Node Detailed Status to bytes");
                        let _ = sender.send(Err(e.into()));
                    }
                }
			},
        }
    }

    // async fn handle_peer_disconnection(&mut self, disconnected_peer_id: PeerId) {
    // 	self.swarm.behaviour_mut().kademlia.get_closest_peers(disconnected_peer_id);
    // }
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
struct L1xProtocol {
    eth_chain_id: Option<u64>,
    cluster_address: Option<String>,
}

impl L1xProtocol {
    fn new(eth_chain_id: Option<u64>, cluster_address: Option<String>) -> Self {
        Self { eth_chain_id, cluster_address }
    }
}

impl ProtocolName for L1xProtocol {
    fn protocol_name(&self) -> &[u8] {
        static LEGACY_PROTOCOL: &[u8] = b"/l1x/protocol/1.0.0";
        
        let protocol = match (self.eth_chain_id, &self.cluster_address) {
            (Some(eth_chain_id), Some(cluster_address)) => {
                debug!("🔍 Network - Protocol | Generating protocol name with chain_id: {} and cluster: {}", eth_chain_id, cluster_address);
                Box::leak(format!("/l1x/{}/{}/protocol/1.0.0", eth_chain_id, cluster_address).into_bytes().into_boxed_slice())
            }
            (Some(eth_chain_id), None) => {
                debug!("🔍 Network - Protocol | Generating protocol name with chain_id: {} (no cluster)", eth_chain_id);
                Box::leak(format!("/l1x/{}/protocol/1.0.0", eth_chain_id).into_bytes().into_boxed_slice())
            }
            _ => {
                debug!("🔍 Network - Protocol | Using legacy protocol name");
                LEGACY_PROTOCOL
            }
        };
        debug!("🔍 Network - Protocol | Final protocol name: {}", String::from_utf8_lossy(protocol));
        protocol
    }
}

#[async_trait]
impl libp2p::request_response::Codec for L1xCodec {
    type Protocol = L1xProtocol;
    type Request = L1xRequest;
    type Response = L1xResponse;

    async fn read_request<T>(&mut self, _: &Self::Protocol, socket: &mut T) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut response = Vec::new();
        // Add size limit for request
        let mut socket = socket.take(Self::MAX_REQUEST_SIZE);
        socket.read_to_end(&mut response).await?;
        deserialize(&response)
    }

    async fn read_response<T>(&mut self, _: &Self::Protocol, socket: &mut T) -> io::Result<Self::Response>
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

fn autonat_behaviour(
    local_peer_id: PeerId,
    autonat_config: &Option<system::config::AutonatConfig>,
) -> autonat::Behaviour {
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

    autonat::Behaviour::new(local_peer_id, config)
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

    let mut gossipsub = gossipsub::Behaviour::new(gossipsub::MessageAuthenticity::Signed(local_key), gossipsub_config)
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

	BroadcastNodeDetailedStatus {
		node_detailed_status: NodeDetailedStatus,
        sender: oneshot::Sender<Result<MessageId, Box<dyn Error + Send>>>,
	}
}



/// Events that are supported to send to the event_receiver for processing
/// ex: Receiving a new transaction from a peer node so it is sent to the event_receiver
/// where it is then validated and added to the mempool.
#[derive(Debug)]
pub enum Event {
    InboundNodeInfo {
        node_info: NodeInfo,
    },
    InboundTransaction {
        transaction: Transaction,
    },
    InboundValidateBlock {
        block_payload: BlockPayload,
    },
    InboundBlockProposer {
        block_proposer_payload: BlockProposerPayload,
    },
    InboundVote {
        vote: Vote,
    },
    InboundVoteResult {
        vote_result: VoteResult,
    },
    InboundQueryBlockResponse {
        block_payload: BlockPayload,
        is_finalized: bool,
        vote_result: Option<VoteResult>,
    },
    InboundQueryBlockRequest {
        request: QueryBlockMessage,
        channel: ResponseChannel<L1xResponse>,
    },
    AggregateNodeHealth {
        epoch: Epoch,
    },
    ProcessNodeHealth {
        epoch: Epoch,
    },
    InboundNodeHealth {
        node_healths: Vec<NodeHealth>,
    },
    InboundAggregatedNodeHealth {
        aggregated_healths: Vec<NodeHealthPayload>,
    },
    PingResult {
        peer_id: String,
        is_success: bool,
        rtt: u64,
    },
    PingEligiblePeers {
        epoch: Epoch,
        peer_ids: Vec<String>,
    },
    CheckNodeStatus,
    InitializePeers,
	PublishNodeDetailedStatus {
        peer_id: String,
    },
    InboundNodeDetailedStatus(NodeDetailedStatus),
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
