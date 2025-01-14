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

	pub const MINT_MASTER_ADDRESS: Address = hex!("78e044394595d4984f66c1b19059bc14ecc24063");
	pub const FEE_RECIPIENT_MASTER_ADDRESS: Address = hex!("7b7ab20f75b691e90c546e89e41aa23b0a821444");
	pub const MULTISIG_DEFAULT_APPROVERS: [Address; 2] = [
		hex!("78e044394595d4984f66c1b19059bc14ecc24063"),
		hex!("7b7ab20f75b691e90c546e89e41aa23b0a821444"),
	];

	pub const SLOTS_PER_EPOCH: u128 = 30;
}

#[cfg(all(feature = "testnet", feature = "mainnet"))]
compile_error!("\"testnet\" and \"mainnet\" features can't be enabled at the same time");

#[cfg(all(feature = "testnet", feature = "devnet"))]
compile_error!("\"testnet\" and \"devnet\" features can't be enabled at the same time");

#[cfg(all(feature = "mainnet", feature = "devnet"))]
compile_error!("\"mainnet\" and \"devnet\" features can't be enabled at the same time");
