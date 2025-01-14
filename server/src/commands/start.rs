use std::{fs::read_to_string, path::PathBuf};
use std::sync::Arc;
use std::time::Duration;

use directories::UserDirs;
use log::{error, info};
use structopt::StructOpt;
use tokio::{sync::mpsc, task, time};
use tokio::sync::{broadcast, RwLock};
use tokio::time::Instant;

use db::db::Database;
use genesis::genesis::Genesis;
use node_crate::node::{FullNode, NodeType};
use primitives::Address;
use system::{config::Config, mempool::ResponseMempool};
use system::config::Db;
use system::network::EventBroadcast;

use crate::{grpc::run_server as grpc_server, json::run_server as json_server};
use crate::snapshot::upload_db_dump_to_s3;

use super::super::service::FullNodeService;

#[derive(Debug, StructOpt)]
#[structopt(name = "start")]
pub struct StartCmd {
    #[structopt(long = "path", short = "w")]
    working_dir: Option<PathBuf>,
    #[structopt(long = "node_type", short = "t", default_value = "full")]
    node_type: NodeType,

}


impl StartCmd {
    pub async fn execute(&self) {
        pretty_env_logger::init();

        let working_dir = match &self.working_dir {
            Some(dir) => dir.clone(),
            None => {
                let user_dirs = UserDirs::new().expect("Couldn't fetch home directory");
                user_dirs.home_dir().to_path_buf()
            }
        };
        println!("Working directory: {:?}", working_dir);

        if !working_dir.exists() {
            println!("l1x folder does not exist");
            return;
        }

        // Construct path to config.toml and genesis.json
        let mut config_path = working_dir.clone();
        config_path.push("config.toml");

        let mut genesis_path = working_dir.clone();
        genesis_path.push("genesis.json");

        // Read and parse config.toml
        let mut parsed_config: Config = Config::default();
        let mut parsed_genesis: Option<Genesis> = None;
        match read_to_string(&config_path) {
            Ok(contents) => match toml::from_str::<Config>(&contents) {
                Ok(config) => {
                    parsed_config = config;
                }
                Err(e) => println!("Could not parse config.toml: {:?}", e),
            },
            Err(e) => println!("Could not read config.toml: {:?}", e),
        }

        // Read and parse genesis.json
        match read_to_string(&genesis_path) {
            Ok(contents) => match serde_json::from_str::<Genesis>(&contents) {
                Ok(genesis) => {
                    parsed_genesis = Some(genesis);
                }
                Err(e) => println!("Could not parse genesis.json: {:?}", e),
            },
            Err(e) => println!("Could not read genesis.json: {:?}", e),
        }

        Database::new(&parsed_config).await;

        let boot_nodes = resolve_boot_nodes(&parsed_config.boot_nodes);
		let cluster_address = hex::decode(&parsed_config.cluster_address)
			.expect("unable to decode cluster address")
			.try_into()
			.expect("Wrong length of Vec");
		let sync_batch_size = parsed_config.sync.batch_size;

		// Initialize the new node
		let (full_node, mempool_res_rx) = FullNode::new(
			parsed_config.max_size,
			parsed_config.fee_limit as u128,
			parsed_config.rate_limit,
			parsed_config.time_frame_seconds as u128,
			parsed_config.expiration_seconds as u128,
			parsed_config.dev_mode,
			parsed_config.multinode_mode,
			&parsed_config.node_ip_address,
			Some(parsed_config.node_private_key.clone()),
			&boot_nodes.iter().map(|s| s.as_str()).collect::<Vec<&str>>(),
			&parsed_config.autonat,
			sync_batch_size,
			parsed_config.block_time.into(),
			cluster_address,
			hex::decode(parsed_config.validator_pool_address).expect("unable to decode cluster address").try_into().expect("Wrong length of Vec"),
			&self.node_type,
            parsed_config.initial_epoch,
		).await;
        // Create a mutex wrapped in an Arc to share across tasks
        let sync_node_guard = Arc::new(RwLock::new(()));
        // Clone the Arc for each task
        let file_mutex = Arc::clone(&sync_node_guard);

        let service = FullNodeService {
            node: full_node.clone(),
            eth_chain_id: parsed_config.eth_chain_id,
            rpc_disable_estimate_fee: parsed_config.rpc_disable_estimate_fee,
        };
        let (mempool_grpc_tx, mempool_grpc_rx) = mpsc::channel(1000);
        let (mempool_json_tx, mempool_json_rx) = mpsc::channel(1000);
        if self.node_type == NodeType::Full {
            info!("Starting full node");
            task::spawn(Self::mempool_response(mempool_res_rx, mempool_grpc_tx, mempool_json_tx));
        } else {
            info!("Starting archive node");
            if !boot_nodes.is_empty() {
                // creating snapshot periodically
                task::spawn(Self::sync_with_boot_node(parsed_config.sync_node_time_interval,
                                                      sync_batch_size,
                                                      boot_nodes,
                                                      cluster_address,
                                                      full_node.node_evm_event_tx,
                                                      sync_node_guard));
                if let Db::Postgres { host, username, password, pool_size, db_name, test_db_name } = parsed_config.db {
                    let database_url = format!(
                        "postgres://{}:{}@{}/{}",
                        username, password, host, db_name
                    );
                    // Skipping snapshotting in case of secondary archive
                    if !parsed_config.disable_snapshot {
                        task::spawn(upload_db_dump_to_s3(parsed_config.snapshot_time_interval, cluster_address, database_url, parsed_config.node_private_key, file_mutex));
                    }
                } else {
                    panic!("Invalid db config.(Required postgres config)");
                }

            } else {
                panic!("No boot node provided");
            }
        }
        info!("Starting rpc servers");
        let grpc_task = task::spawn(grpc_server(
            parsed_config.grpc,
            parsed_config.grpc_port.clone(),
            service.clone(),
            mempool_grpc_rx,
        ));
        let json_rpc_task = task::spawn(json_server(parsed_config.jsonrpc_port.clone(), service, mempool_json_rx));

        // exit when either task finishes
        tokio::select! {
			res = grpc_task => info!("GRPC exited: {:?}", res),
			res = json_rpc_task => info!("JSON exited: {:?}", res),
		}
    }

    pub async fn mempool_response(
        mut mempool_rx: mpsc::Receiver<ResponseMempool>,
        mempool_grpc_tx: mpsc::Sender<ResponseMempool>,
        mempool_json_tx: mpsc::Sender<ResponseMempool>,
    ) {
        while let Some(mempool_response) = mempool_rx.recv().await {
            // The below code is commented out becaise grpc and json channels are never read and it causes the bug:
            // https://github.com/L1X-Foundation-Consensus/l1x-consensus/issues/821

            // if let Err(e) = mempool_grpc_tx.send(mempool_response.clone()).await {
            //     error!("Unable to write mempool_response to mempool_grpc_tx channel: {:?}", e);
            // }
            // if let Err(e) = mempool_json_tx.send(mempool_response.clone()).await {
            //     error!("Unable to write mempool_response to mempool_json_tx channel: {:?}", e);
            // }
        }
    }

    pub async fn sync_with_boot_node(time_interval: u64,
                                     batch_size: u16,
									 boot_nodes: Vec<String>,
									 cluster_address: Address,
									 event_tx: broadcast::Sender<EventBroadcast>,
									 sync_node_guard: Arc<RwLock<()>>,
	) {
        let duration = Duration::from_secs(time_interval);
        // Create a timer that fires at specified intervals
        let mut interval = time::interval(duration);
        loop {
            // Wait until the next interval
            interval.tick().await;
            info!("Starting node syncing...");
            let boot_nodes: &[&str] = &boot_nodes.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            let sync_start_time = Instant::now();
            // Lock the mutex before writing to the file
            let guard = sync_node_guard.read().await;
            // Sync historical blocks from existing archive/full node
            match node_crate::sync::sync_node(cluster_address, boot_nodes, event_tx.clone(), batch_size).await {
                Ok(_) => {
                    info!("✅ Syncing node successful ✅");
                    info!(
					"⌛️ Syncing node took: {:?} seconds",
					sync_start_time.elapsed().as_secs()
				);
                }
                Err(e) => {
                    error!("Unable to sync node: {:?}", e);
                }
            }
            drop(guard);
        }
    }
}

fn resolve_boot_nodes(multiaddrs: &Vec<String>) -> Vec<String> {
    // /ip4/172.30.0.6/tcp/5010/p2p/16Uiu2HAm5cnMkuwwmNyNdHNzxvhtHxKQ1xJtF7zb5Fpm9zbe7qmm
    let re_hostname = regex::Regex::new(r"/ip4/([^/]+)/tcp/(\d+)/p2p/([a-zA-Z\d]+)").unwrap();
    let re_ipaddress = regex::Regex::new(r"\d+.\d+.\d+.\d+").unwrap();

    let mut result = Vec::new();
    for addr in multiaddrs {
        if let Some(capture) = re_hostname.captures(addr) {
            let hostname = capture.get(1).unwrap();
            if re_ipaddress.captures(hostname.as_str()).is_none() {
                let port = capture.get(2).unwrap().as_str();
                let peer_id = capture.get(3).unwrap().as_str();
                let addrs = dns_lookup::lookup_host(hostname.as_str()).unwrap();
                if let Some(ip_addr) = addrs.iter().find(|v| v.is_ipv4() && !v.is_loopback() && !v.is_multicast() && !v.is_unspecified()) {
                    let multiaddr = format!("/ip4/{}/tcp/{}/p2p/{}", ip_addr.to_string(), port, peer_id);
                    result.push(multiaddr);
                } else {
                    log::warn!("Can't resolve {} hostname", hostname.as_str());
                }
            } else {
                result.push(addr.clone())
            }
        } else {
            log::warn!("Inocorred boot node url format: {}", addr)
        }
    }

    log::info!("Boot nodes:");
    result.iter().for_each(|a| {
        log::info!("* {a}")
    });

    result
}
