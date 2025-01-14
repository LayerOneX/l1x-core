use crate::account::Account;
use anyhow::{anyhow, Error};
use primitives::MemPoolSize;
use regex::Regex;
use secp256k1::{Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use std::{fmt, fs::read_to_string, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

/// Cached configuration
lazy_static::lazy_static! {
    pub static ref CACHED_CONFIG: Arc<RwLock<Option<Arc<Config>>>> = Arc::new(RwLock::new(None));
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AutonatConfig {
	pub timeout: Option<u64>,
	pub boot_delay: Option<u64>,
	pub refresh_interval: Option<u64>,
	pub retry_interval: Option<u64>,
	pub throttle_server_period: Option<u64>,
	pub confidence_max: Option<usize>,

	pub max_peer_addresses: Option<usize>,
	pub throttle_clients_global_max: Option<usize>,
	pub throttle_clients_peer_max: Option<usize>,
	pub throttle_clients_period: Option<u64>,
	pub only_global_ips: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GrpcConfig {
	pub max_concurrent_streams: Option<u32>,
	pub concurrency_limit_per_connection: Option<usize>,
	pub block_cache_capacity: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncConfig {
	#[serde(default = "default_batch_size")]
	pub batch_size: u16,
}

impl Default for SyncConfig {
	fn default() -> Self {
		Self {
			batch_size: 50,
		}
	}
}

fn default_batch_size() -> u16 {
	SyncConfig::default().batch_size
}

fn default_sync_config() -> SyncConfig {
	SyncConfig::default()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InitialEpochConfig {
	pub block_proposer: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Db {
	#[serde(alias = "Postgres", alias = "postgres")]
	Postgres {
		host: String,
		username: String,
		password: String,
		pool_size: u32,
		db_name: String,
		test_db_name: Option<String>
	},
	#[serde(alias = "Cassandra", alias = "cassandra")]
	Cassandra {
		host: String,
		username: String,
		password: String,
		keyspace: String,
	},
	#[serde(alias = "RocksDb", alias = "rocksdb")]
	RocksDb {
		name: String,
	}
}

/// Startup configuration for running an L1X node
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
	pub weight_for_stakescore: Option<f64>,
	pub weight_for_kinscore: Option<f64>,
	pub xscore_threshold: Option<f64>,
	pub eth_chain_id: u64,
	pub dev_mode: bool,
	pub multinode_mode: bool,
	pub replication_enable: bool,
	pub max_size: MemPoolSize,
	pub fee_limit: u64,
	pub rate_limit: usize,
	pub time_frame_seconds: u64,
	pub expiration_seconds: u64,
	pub block_time: u64,
	pub node_port: String,
	pub node_ip_address: String, // eg. "/ip4/0.0.0.0/tcp/5010"
	pub node_private_key: String,
	pub node_verifying_key: String,
	pub node_address: String,
	pub node_metadata: String,
	pub cluster_address: String,
	pub validator_pool_address: String,
	pub boot_nodes: Vec<String>, // eg. &["/ip4/0.0.0.0/tcp/5010/p2p/1234567890"]
	pub archive_nodes: Vec<String>,
	pub archive_public_key: String,
	pub sync_node_time_interval: u64,
	pub snapshot_time_interval: u64,
	pub grpc_port: String,
	pub disable_snapshot: bool,
	pub jsonrpc_port: String,
	pub rpc_disable_estimate_fee: bool,
	pub autonat: Option<AutonatConfig>,
	pub initial_epoch: Option<InitialEpochConfig>,
	pub grpc: Option<GrpcConfig>,
	#[serde(default = "default_sync_config")]
	pub sync: SyncConfig,
	pub db: Db,
}

impl Default for Config {
	fn default() -> Self {
		let default_config = Self::default_config().expect("Error reading default_config");
		let node_private_key = default_config.node_private_key;
		let secret_key = SecretKey::from_slice(
			&hex::decode(node_private_key.clone()).expect("Error decoding node_private_key"),
		)
		.expect("Failed to parse provided private_key");
		let secp = Secp256k1::new();
		let verifying_key = secret_key.public_key(&secp);
		let verifying_key_bytes = verifying_key.serialize().to_vec();
		let node_address =
			Account::address(&verifying_key_bytes.clone()).expect("Failed to get node address");

		Self {
			weight_for_stakescore: default_config.weight_for_stakescore,
			weight_for_kinscore: default_config.weight_for_kinscore,
			xscore_threshold: default_config.xscore_threshold,
			eth_chain_id: default_config.eth_chain_id,
			replication_enable: default_config.replication_enable,
			dev_mode: default_config.dev_mode,
			multinode_mode: default_config.multinode_mode,
			max_size: default_config.max_size,
			fee_limit: default_config.fee_limit,
			rate_limit: default_config.rate_limit,
			time_frame_seconds: default_config.time_frame_seconds,
			expiration_seconds: default_config.expiration_seconds,
			block_time: default_config.block_time,
			node_port: default_config.node_port,
			node_ip_address: default_config.node_ip_address,
			node_private_key,
			node_verifying_key: hex::encode(verifying_key_bytes).to_string(),
			node_address: hex::encode(node_address).to_string(),
			node_metadata: default_config.node_metadata,
			cluster_address: default_config.cluster_address,
			boot_nodes: default_config.boot_nodes,
			archive_nodes: default_config.archive_nodes,
			archive_public_key: default_config.archive_public_key,
			sync_node_time_interval: default_config.sync_node_time_interval,
			snapshot_time_interval: default_config.snapshot_time_interval,
			disable_snapshot: default_config.disable_snapshot,
			grpc_port: default_config.grpc_port,
			jsonrpc_port: default_config.jsonrpc_port,
			validator_pool_address: default_config.validator_pool_address,
			rpc_disable_estimate_fee: false,
			autonat: default_config.autonat,
			initial_epoch: default_config.initial_epoch,
			grpc: default_config.grpc,
			sync: default_config.sync,
			db: default_config.db,
		}
	}
}

impl Config {
	/// Create a new configuration instance and store it in CACHED_CONFIG.
	pub async fn new(config: Config) {
		let mut lock = CACHED_CONFIG.write().await; // Acquire a write lock
		*lock = Some(Arc::new(config.clone()));
	}

	pub async fn get_config() -> Result<Config, Error> {
		let lock = CACHED_CONFIG.read().await;
		if let Some(config) = &*lock {
			Ok(<Config as Clone>::clone(&(*Arc::clone(config))))
		} else {
			Err(anyhow!("Config not initialized!"))
		}
	}
	fn default_config() -> Result<Config, Error> {
		// Get the current directory
		let current_dir = std::env::current_dir()?;
		//println!("Working directory: {:?}", current_dir);

		// Define the regular expression pattern to match "l1x-consensus" or its subdirectories
		let pattern = Regex::new(r"^(.*/l1x-consensus)(/.*)?$")?;

		// Check if the current directory matches the pattern
		let current_dir =
			if let Some(captures) = pattern.captures(current_dir.to_str().expect("Invalid path")) {
				if let Some(parent_dir) = captures.get(1) {
					// If there's a subdirectory, append it to the path
					let mut new_path = PathBuf::new();
					new_path.push(parent_dir.as_str());
					//println!("Trimmed path: {:?}", new_path);
					new_path
				} else {
					// If there's no subdirectory, keep the path as is
					//println!("Current directory matches: {:?}", current_dir);
					current_dir
				}
			} else {
				// If the current directory doesn't match, print a message
				//println!("Current directory does not match the pattern: {:?}", current_dir);
				current_dir
			};

		// Construct path to config.toml and genesis.json
		let mut config_path = current_dir.clone();
		config_path.push("config.toml");

		// Read and parse config.toml
		match read_to_string(&config_path) {
			Ok(contents) => match toml::from_str::<Config>(&contents) {
				Ok(config) => Ok(config),
				Err(e) =>
					Err(anyhow!("Could not parse '{}': {:?}", config_path.to_string_lossy(), e)),
			},
			Err(e) => Err(anyhow!("Could not read '{}': {:?}", config_path.to_string_lossy(), e)),
		}
	}
}
impl fmt::Display for Config {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		// You can customize this to print your struct as you see fit.
		// The example just uses Debug for simplicity.
		write!(f, "{:?}", self)
	}
}
