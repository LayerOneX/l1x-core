use hex_literal::hex;
use primitives::{Address, Balance, BlockNumber};


pub const BLOCK_VERSION: u32 = 5;
pub const PROTOCOL_VERSION: u32 = 3;
pub const HEALTH_VERSION: u32 = 1;
pub const DEFAULT_XSCORE_THRESHOLD: f64 = 0.65;
pub const DEFAULT_MAX_VALIDATORS: u64 = 2;
pub const ANOMALY_THRESHOLD: f64 = 0.5;
pub const SEVERE_ANOMALY_THRESHOLD: f64 = 0.8;

pub const NODE_STATUS_TIMER: u64 = 10;

pub const ELIGIBLE_PEERS_INIT_BLOCK_NUMBER: BlockNumber = 10;

pub const GAS_PRICE: Balance = 7000; // 7000 Gas for 1 nanoL1X
pub const EVM_GAS_PRICE: Balance = 7000; // 7000 Gas for 1 nanoL1X, Not used.

pub const SYSTEM_CONTRACTS_OWNER: Address = hex!("ff00000000000000000000000000000000000000");
pub const SYSTEM_REWARDS_DISTRIBUTOR: Address = hex!("ff00000000000000000000000000000000000001");

pub const MAX_SYNC_BLOCK_DIFF: u128 = 9000; // 9000 blocks

pub const MAX_BLOCK_DIFF: u64 = 50; // 50 blocks for Validator selection
pub mod voting_config {
	pub const VOTE_THRESHOLD: f64 = 0.60; // 60%
	pub const STAKE_PASS_NUMERATOR: u64 = 60;
	pub const STAKE_PASS_DENOMINATOR: u64 = 100;
	pub const BLOCK_EXPIRATION_TIME: u128 = 20_000; // 30 seconds

	pub const PHASE_1_TIMEOUT: u128 = 10_000;  // 10 seconds
	pub const PHASE_2_TIMEOUT: u128 = 15_000;  // 15 seconds
	pub const PHASE_3_TIMEOUT: u128 = 20_000;  // 20 seconds

	// Threshold reduction factors (applied to VOTE_THRESHOLD)
	pub const PHASE_2_THRESHOLD_FACTOR: f64 = 0.8;  // 80% of normal threshold (56% if VOTE_THRESHOLD is 70%)
	pub const PHASE_4_THRESHOLD_FACTOR: f64 = 0.8;  // 80% of normal threshold (56% if VOTE_THRESHOLD is 70%)
}

mod mainnet_config {
	use hex_literal::hex;
	use primitives::Address;

	pub const MAINNET_MINT_MASTER_ADDRESS: Address = hex!("bd9641b7A6c7FD137dD75dA9a129965754c5620a");
	pub const MAINNET_FEE_RECIPIENT_MASTER_ADDRESS: Address = hex!("f1bac54594b120f468108652e7669791f3e7aaf1");
	pub const MAINNET_MULTISIG_DEFAULT_APPROVERS: [Address; 2] = [
		hex!("21f05ed3e1be2b2067a251125fef50db2f97f91a"),
		hex!("caaa9106183068622d3db91c67562811a320653b"),
	];

	pub const MAINNET_SLOTS_PER_EPOCH: u128 = 100;
}

// Default case
#[cfg(not(any(feature = "testnet", feature = "mainnet", feature = "devnet")))]
pub mod config {
	use primitives::Address;

	pub const MINT_MASTER_ADDRESS: Address = crate::mainnet_config::MAINNET_MINT_MASTER_ADDRESS;
	pub const FEE_RECIPIENT_MASTER_ADDRESS: Address = crate::mainnet_config::MAINNET_FEE_RECIPIENT_MASTER_ADDRESS;
	pub const MULTISIG_DEFAULT_APPROVERS: [Address; 2] = crate::mainnet_config::MAINNET_MULTISIG_DEFAULT_APPROVERS;

	pub const SLOTS_PER_EPOCH: u128 = crate::mainnet_config::MAINNET_SLOTS_PER_EPOCH;
}

// Testnet or mainnet
#[cfg(any(feature = "testnet", feature = "mainnet"))]
pub mod config {
	use primitives::Address;

	pub const MINT_MASTER_ADDRESS: Address = crate::mainnet_config::MAINNET_MINT_MASTER_ADDRESS;
	pub const FEE_RECIPIENT_MASTER_ADDRESS: Address = crate::mainnet_config::MAINNET_FEE_RECIPIENT_MASTER_ADDRESS;
	pub const MULTISIG_DEFAULT_APPROVERS: [Address; 2] = crate::mainnet_config::MAINNET_MULTISIG_DEFAULT_APPROVERS;

	pub const SLOTS_PER_EPOCH: u128 = crate::mainnet_config::MAINNET_SLOTS_PER_EPOCH;
}

// devnet
#[cfg(feature = "devnet")]
pub mod config {
	use hex_literal::hex;
	use primitives::Address;

	pub const MINT_MASTER_ADDRESS: Address = hex!("bdba6171ff1f7fe74c20acd40c55ba26cdf4310a");
	pub const FEE_RECIPIENT_MASTER_ADDRESS: Address = hex!("bdba6171ff1f7fe74c20acd40c55ba26cdf4310a");
	pub const MULTISIG_DEFAULT_APPROVERS: [Address; 2] = [
		hex!("bdba6171ff1f7fe74c20acd40c55ba26cdf4310a"),
		hex!("451535eb42068d42f41873413e6f7ae7dc56c6af"),
	];

	pub const SLOTS_PER_EPOCH: u128 = crate::mainnet_config::MAINNET_SLOTS_PER_EPOCH;
}

#[cfg(all(feature = "testnet", feature = "mainnet"))]
compile_error!("\"testnet\" and \"mainnet\" features can't be enabled at the same time");

#[cfg(all(feature = "testnet", feature = "devnet"))]
compile_error!("\"testnet\" and \"devnet\" features can't be enabled at the same time");

#[cfg(all(feature = "mainnet", feature = "devnet"))]
compile_error!("\"mainnet\" and \"devnet\" features can't be enabled at the same time");


#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub mod network_namespace {
	pub const NETWORK_NAMESPACE_ID: &str = "mainnet-d310d666d3c35201044b1";
}

#[cfg(feature = "testnet")]
pub mod network_namespace {
	pub const NETWORK_NAMESPACE_ID: &str = "testnet-80a11efa9d8c5b3a94934";
}

#[cfg(feature = "devnet")]
pub mod network_namespace {
	pub const NETWORK_NAMESPACE_ID: &str = "devnet-f95f8a887cf1521302693";
}


#[cfg(not(any(feature = "testnet", feature = "devnet")))] // Corresponds to mainnet
pub mod p2p_topics {
	pub const NODE_INFO_TOPIC: &str = "mainnet-d310d666d3c35201044b1:node_join";
	pub const TRANSACTIONS_TOPIC: &str = "mainnet-d310d666d3c35201044b1:transactions";
	pub const BLOCKS_VALIDATE_TOPIC: &str = "mainnet-d310d666d3c35201044b1:blocks_validate";
	pub const BLOCK_PROPOSER_TOPIC: &str = "mainnet-d310d666d3c35201044b1:block_proposers";
	pub const VOTE_TOPIC: &str = "mainnet-d310d666d3c35201044b1:vote";
	pub const VOTE_RESULT_TOPIC: &str = "mainnet-d310d666d3c35201044b1:vote_result";
	pub const NODE_HEALTH_TOPIC: &str = "mainnet-d310d666d3c35201044b1:node_health";
	pub const AGGREGATED_NODE_HEALTH_TOPIC: &str = "mainnet-d310d666d3c35201044b1:aggregate_node_health";
	pub const BROADCAST_NODE_DETAILED_STATUS_TOPIC: &str = "mainnet-d310d666d3c35201044b1:broadcast_node_detailed_status";
	pub const VALIDATORS_TOPIC: &str = "mainnet-d310d666d3c35201044b1:validators";
}

#[cfg(feature = "testnet")]
pub mod p2p_topics {
	pub const NODE_INFO_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:node_join";
	pub const TRANSACTIONS_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:transactions";
	pub const BLOCKS_VALIDATE_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:blocks_validate";
	pub const BLOCK_PROPOSER_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:block_proposers";
	pub const VOTE_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:vote";
	pub const VOTE_RESULT_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:vote_result";
	pub const NODE_HEALTH_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:node_health";
	pub const AGGREGATED_NODE_HEALTH_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:aggregate_node_health";
	pub const BROADCAST_NODE_DETAILED_STATUS_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:broadcast_node_detailed_status";
	pub const VALIDATORS_TOPIC: &str = "testnet-80a11efa9d8c5b3a94934:validators";
}

#[cfg(feature = "devnet")]
pub mod p2p_topics {
	pub const NODE_INFO_TOPIC: &str = "devnet-f95f8a887cf1521302693:node_join";
	pub const TRANSACTIONS_TOPIC: &str = "devnet-f95f8a887cf1521302693:transactions";
	pub const BLOCKS_VALIDATE_TOPIC: &str = "devnet-f95f8a887cf1521302693:blocks_validate";
	pub const BLOCK_PROPOSER_TOPIC: &str = "devnet-f95f8a887cf1521302693:block_proposers";
	pub const VOTE_TOPIC: &str = "devnet-f95f8a887cf1521302693:vote";
	pub const VOTE_RESULT_TOPIC: &str = "devnet-f95f8a887cf1521302693:vote_result";
	pub const NODE_HEALTH_TOPIC: &str = "devnet-f95f8a887cf1521302693:node_health";
	pub const AGGREGATED_NODE_HEALTH_TOPIC: &str = "devnet-f95f8a887cf1521302693:aggregate_node_health";
	pub const BROADCAST_NODE_DETAILED_STATUS_TOPIC: &str = "devnet-f95f8a887cf1521302693:broadcast_node_detailed_status";
	pub const VALIDATORS_TOPIC: &str = "devnet-f95f8a887cf1521302693:validators";
}