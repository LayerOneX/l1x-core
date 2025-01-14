use db::db::Database;
use directories::UserDirs;
use libp2p::identity::Keypair;
use node_info::{node_info_manager::NodeInfoManager, node_info_state::NodeInfoState};
use primitives::Address;
use std::{fs::read_to_string, path::PathBuf};
use structopt::StructOpt;
use system::config::Config;
use secp256k1::{Secp256k1, SecretKey};
use std::{
	fs::File,
	io::Write,
};

use block::{block_manager::BlockManager, block_state::BlockState};

#[derive(Debug, StructOpt)]
#[structopt(name = "update-node-info")]
pub struct UpdateNodeInfoCmd {
	#[structopt(long = "path", short = "w")]
	working_dir: Option<PathBuf>,
}

impl UpdateNodeInfoCmd {
	pub async fn execute(&self) {
		pretty_env_logger::init();

		let working_dir = match &self.working_dir {
			Some(dir) => dir.clone(),
			None => {
				let user_dirs = UserDirs::new().expect("Couldn't fetch home directory");
				user_dirs.home_dir().to_path_buf()
			},
		};
		println!("Working directory: {:?}", working_dir);

		if !working_dir.exists() {
			println!("l1x folder does not exist");
			return;
		}

		// Construct path to config.toml and genesis.json
		let mut config_path = working_dir.clone();
		config_path.push("config.toml");

		// Read and parse config.toml
		let mut parsed_config: Config = Config::default();
		match read_to_string(&config_path) {
			Ok(contents) => match toml::from_str::<Config>(&contents) {
				Ok(config) => {
					parsed_config = config;
				},
				Err(e) => println!("Could not parse config.toml: {:?}", e),
			},
			Err(e) => println!("Could not read config.toml: {:?}", e),
		}

		Database::new(&parsed_config).await;

		let cluster_address: Address = hex::decode(parsed_config.cluster_address.clone())
			.expect("unable to decode cluster address")
			.try_into()
			.expect("Wrong length of Vec");
		let node_address: Address = hex::decode(parsed_config.node_address.clone())
			.expect("unable to decode node address")
			.try_into()
			.expect("Wrong length of Vec");


		let secret_key = SecretKey::from_slice(
			&hex::decode(parsed_config.node_private_key.clone())
				.expect("Error decoding node_private_key"),
		)
		.expect("Error parsing into SecretKey");
		let secp = Secp256k1::new();
		let verifying_key = secret_key.public_key(&secp);
		let verifying_key_bytes = verifying_key.serialize().to_vec();

		// Derive PeerId from the public key
		let node_keypair = Some(parsed_config.node_private_key.clone()).as_ref().map_or_else(
			|| Keypair::generate_secp256k1(),
			|private_key| {
				let mut keypair_bytes =
					hex::decode(private_key).expect("Failed to hex decode privkey");
				let secret_key =
					libp2p::identity::secp256k1::SecretKey::try_from_bytes(&mut keypair_bytes)
						.expect("Failed to parse keypair");

				// Create a new Keypair using the secp256k1::Keypair constructor
				let secp_keypair = libp2p::identity::secp256k1::Keypair::from(secret_key);

				// Use try_into_secp256k1 from libp2p_identity to convert to Keypair
				let keypair: libp2p::identity::Keypair =
					secp_keypair.try_into().expect("Failed to convert to Keypair");

				keypair
			},
		);
		let peer_id = node_keypair.public().to_peer_id().to_string();
		parsed_config.node_verifying_key = hex::encode(verifying_key_bytes.clone());
		let block_manager = BlockManager{};
		let db_pool_conn = Database::get_pool_connection().await.expect("Unable to get db conn");
		{
			let state = NodeInfoState::new(&db_pool_conn).await.expect("Error creating NodeInfoState");
			
			if state.remove_node_info(&node_address.clone()).await.is_ok() {
				println!("Old Node Info for address '{}' has been removed", hex::encode(node_address));
			}
			let block_state = BlockState::new(&db_pool_conn).await.expect("failed to load block state");
			let current_block_height = block_state.block_head_header(cluster_address).await.expect("failed to get block height").block_number;
			let joined_epoch = match block_manager.calculate_current_epoch(current_block_height) {
				Ok(epoch) => epoch as u64,
				Err(e) => {
					log::error!("Unable to get current_epoch: {:?}", e);
					0u64
				}
			};
			NodeInfoManager::store_node_info(
				node_address.clone(),
				parsed_config.node_ip_address.as_bytes().to_vec(),
				peer_id.to_string(),
				parsed_config.node_metadata.as_bytes().to_vec(),
				cluster_address.clone(),
				joined_epoch,
				secret_key.clone(),
				verifying_key_bytes.clone(),
				&db_pool_conn,
			)
			.await
			.expect("Unable to store_node_info");

			println!("Node Info has been updated");
			println!("node_address={}\nnode_ip_address={}\nmetadata={}\ncluster_address={}\nsecret_key=...\nverifying_key={}",
				hex::encode(node_address),
				parsed_config.node_ip_address,
				parsed_config.node_metadata,
				hex::encode(cluster_address),
				hex::encode(verifying_key_bytes)
			);
		}

				// Serialize the struct to a TOML string
		let config_str = toml::to_string(&parsed_config).expect("Failed to serialize config");

		let mut file = File::create(&config_path).expect("Couldn't create config file");
		file.write_all(config_str.as_bytes()).expect("Couldn't write to config file");
		println!("TOML config file has been updated at {:?}", config_path);
	}
}