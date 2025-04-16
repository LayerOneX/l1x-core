use std::collections::BTreeSet;

use base64::Engine;
use compile_time_config::config::MULTISIG_DEFAULT_APPROVERS;
use hex_literal::hex;
use primitives::Address;
use serde::Serialize;

pub const MULTISIG_CONTRACT_OBJECT_BYTES: &[u8] = include_bytes!("../contracts_binaries/system_multisig.o");
pub const CONFIG_CONTRACT_OBJECT_BYTES: &[u8] = include_bytes!("../contracts_binaries/system_config.o");
pub const NODE_REGISTRY_CONTRACT_OBJECT_BYTES: &[u8] = include_bytes!("../contracts_binaries/system_node_registry.o");
pub const STAKING_CONTRACT_OBJECT_BYTES: &[u8] = include_bytes!("../contracts_binaries/system_staking.o");
pub const DENYLIST_CONTRACT_OBJECT_BYTES: &[u8] = include_bytes!("../contracts_binaries/system_denylist.o");



// --- DEFAULT (Mainnet) Addresses (0101...) ---
// Used when neither testnet nor devnet features are enabled.
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const CLUSTER_ADDRESS: Address = hex!("0101010101010101010101010101010101010101");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const MULTISIG_CONTRACT_CODE_ADDRESS: Address = hex!("45ea9e24bd75bfd3102c764d8666c548aa6de7bb");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const MULTISIG_CONTRACT_INSTANCE_ADDRESS: Address = hex!("813014b4646086532239fb10a322cf727fa99515");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const CONFIG_CONTRACT_CODE_ADDRESS: Address = hex!("60f2b6cde008c09da6c544b719ce4f2ef653268b");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const CONFIG_CONTRACT_INSTANCE_ADDRESS: Address = hex!("6e47e973e3819126314bdd9e45c7ae5de34f9167");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const NODE_REGISTRY_CONTRACT_CODE_ADDRESS: Address = hex!("b9afacf9d230ea197e730a8f5c748414cddd52c4");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const NODE_REGISTRY_CONTRACT_INSTANCE_ADDRESS: Address = hex!("045decede842279b25defd16f7c1f922634cdce3");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const STAKING_CONTRACT_CODE_ADDRESS: Address = hex!("0d8124d9e25edbe09715ae9c01a41d2c02509817");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const STAKING_CONTRACT_INSTANCE_ADDRESS: Address = hex!("9f10ca806f89c4d057ac80badb3f671219dfd319");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const DENYLIST_CONTRACT_CODE_ADDRESS: Address = hex!("303bfb3faeab042d1e8f2ac194311bb02ac651c4");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub const DENYLIST_CONTRACT_INSTANCE_ADDRESS: Address = hex!("407e17179e40b1535b8ed0145fa0c111daa2cd72");
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub mod default_genesis_attributes {

    use hex_literal::hex;
    use primitives::Address;

    pub const DEFAULT_STAKING_POOL_ADDRESS: Address = hex!("522b3294fe78d57a1d7e1c37393f11841f6a9494");
    pub const DEFAULT_BLOCK_PROPOSER_ADDRESS: Address = hex!("78e044394595d4984f66c1b19059bc14ecc24063");

    pub const DEFAULT_VALIDATOR_1_ADDRESS: Address = hex!("78e044394595d4984f66c1b19059bc14ecc24063");
    pub const DEFAULT_VALIDATOR_2_ADDRESS: Address = hex!("7b7ab20f75b691e90c546e89e41aa23b0a821444");
    pub const DEFAULT_VALIDATOR_3_ADDRESS: Address = hex!("4489da9d81f0bc8125c8efdda1c117a7a895b43d");
}

// --- TESTNET Addresses (0404...) ---
#[cfg(feature = "testnet")]
pub const CLUSTER_ADDRESS: Address = hex!("0404040404040404040404040404040404040404");
#[cfg(feature = "testnet")]
pub const MULTISIG_CONTRACT_CODE_ADDRESS: Address = hex!("dbe414e6d9b1df7c79d3cdf1ff13dd0aac106235");
#[cfg(feature = "testnet")]
pub const MULTISIG_CONTRACT_INSTANCE_ADDRESS: Address = hex!("7536f85cde53ca284acf0a522aac69e0d5d19634");
#[cfg(feature = "testnet")]
pub const CONFIG_CONTRACT_CODE_ADDRESS: Address = hex!("29d1efb5042541ca2f3054ce361808ef4a0abfd9");
#[cfg(feature = "testnet")]
pub const CONFIG_CONTRACT_INSTANCE_ADDRESS: Address = hex!("72f0ea928a2cec76e5061e822d9476b4df2cefad");
#[cfg(feature = "testnet")]
pub const NODE_REGISTRY_CONTRACT_CODE_ADDRESS: Address = hex!("bf4d555a0715b3f8215b60217d8ae742b5deff1f");
#[cfg(feature = "testnet")]
pub const NODE_REGISTRY_CONTRACT_INSTANCE_ADDRESS: Address = hex!("7d7324a64c45abce5cc802cbf7a4574cf32cacb8");
#[cfg(feature = "testnet")]
pub const STAKING_CONTRACT_CODE_ADDRESS: Address = hex!("1d08534009dbf06ade850bdb3a10f0ce69f1be8d");
#[cfg(feature = "testnet")]
pub const STAKING_CONTRACT_INSTANCE_ADDRESS: Address = hex!("46a3bdc98f27c8584d95583a257bcb7153e9df04");
#[cfg(feature = "testnet")]
pub const DENYLIST_CONTRACT_CODE_ADDRESS: Address = hex!("67a4151e326c0522fc2cbb908735f0533cbcb323");
#[cfg(feature = "testnet")]
pub const DENYLIST_CONTRACT_INSTANCE_ADDRESS: Address = hex!("4dd2b4d547cbc75a0fa1bfb1f270d92f4a841d09");

#[cfg(feature = "testnet")]
pub mod default_genesis_attributes {

    use hex_literal::hex;
    use primitives::Address;

    pub const DEFAULT_STAKING_POOL_ADDRESS: Address = hex!("367693cc5ed5055ef407ed556e1f4172c3e81e14");
    pub const DEFAULT_BLOCK_PROPOSER_ADDRESS: Address = hex!("bdba6171ff1f7fe74c20acd40c55ba26cdf4310a");

    pub const DEFAULT_VALIDATOR_1_ADDRESS: Address = hex!("bdba6171ff1f7fe74c20acd40c55ba26cdf4310a");
    pub const DEFAULT_VALIDATOR_2_ADDRESS: Address = hex!("451535eb42068d42f41873413e6f7ae7dc56c6af");
    pub const DEFAULT_VALIDATOR_3_ADDRESS: Address = hex!("a579df673ee67cb0abb6bd3cf9f5b2e2bfc45e33");
}

// --- DEVNET Addresses (0303...) ---
#[cfg(feature = "devnet")]
pub const CLUSTER_ADDRESS: Address = hex!("0303030303030303030303030303030303030303");
#[cfg(feature = "devnet")]
pub const MULTISIG_CONTRACT_CODE_ADDRESS: Address = hex!("76627d0294ebb7b37431d2a93e0a6e5664de663b");
#[cfg(feature = "devnet")]
pub const MULTISIG_CONTRACT_INSTANCE_ADDRESS: Address = hex!("266ca72f239b8fa83b0586785e5da0233f313d9f");
#[cfg(feature = "devnet")]
pub const CONFIG_CONTRACT_CODE_ADDRESS: Address = hex!("223036bf02e17a2a03ab1523b9af1ba1abf0be7d");
#[cfg(feature = "devnet")]
pub const CONFIG_CONTRACT_INSTANCE_ADDRESS: Address = hex!("1c5dcdef26574c63fa8d2b84426aa07c35eb1250");
#[cfg(feature = "devnet")]
pub const NODE_REGISTRY_CONTRACT_CODE_ADDRESS: Address = hex!("4abc1fe2d61db09ffb111af3c3dc74dd28fd59c6");
#[cfg(feature = "devnet")]
pub const NODE_REGISTRY_CONTRACT_INSTANCE_ADDRESS: Address = hex!("88b971856072d18a47e0ddf4a990cbcf9776e4d5");
#[cfg(feature = "devnet")]
pub const STAKING_CONTRACT_CODE_ADDRESS: Address = hex!("31af46a7759d8e2562009f2a59c25246f06655e2");
#[cfg(feature = "devnet")]
pub const STAKING_CONTRACT_INSTANCE_ADDRESS: Address = hex!("c4908ceb718c295497fdc6cdfdae3098b0f36d90");
#[cfg(feature = "devnet")]
pub const DENYLIST_CONTRACT_CODE_ADDRESS: Address = hex!("231fbc9de8ae29ace6f85ec2a0e8e92c4e42ac6f");
#[cfg(feature = "devnet")]
pub const DENYLIST_CONTRACT_INSTANCE_ADDRESS: Address = hex!("e1c5f3cd6ba8ba92006182426d5f586022c764f0");

#[cfg(feature = "devnet")]
pub mod default_genesis_attributes {

    use hex_literal::hex;
    use primitives::Address;

    // TODO : GENERATE PROPER Address
    pub const DEFAULT_STAKING_POOL_ADDRESS: Address = hex!("522b3294fe78d57a1d7e1c37393f11841f6a9494");
    pub const DEFAULT_BLOCK_PROPOSER_ADDRESS: Address = hex!("78e044394595d4984f66c1b19059bc14ecc24063");

    pub const DEFAULT_VALIDATOR_1_ADDRESS: Address = hex!("bdba6171ff1f7fe74c20acd40c55ba26cdf4310a");
    pub const DEFAULT_VALIDATOR_2_ADDRESS: Address = hex!("451535eb42068d42f41873413e6f7ae7dc56c6af");
    pub const DEFAULT_VALIDATOR_3_ADDRESS: Address = hex!("a579df673ee67cb0abb6bd3cf9f5b2e2bfc45e33");
}



#[cfg(not(feature = "devnet"))]
mod config_init_mainnet_data {
    use hex_literal::hex;
    use primitives::Address;

    use crate::GlobalPoolSettingsInfo;

    pub const SYSTEM_CONFIG_INIT_DATA: &[u8] = include_bytes!("../contracts_init_data/system_config_mainnet.toml");
    pub const SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS: [Address; 6] = [
        hex!("78e044394595d4984f66c1b19059bc14ecc24063"),
        hex!("7b7ab20f75b691e90c546e89e41aa23b0a821444"),
        hex!("4489da9d81f0bc8125c8efdda1c117a7a895b43d"),
        hex!("3b647b46c9ba4fca221ccf933c09b653c2b4581f"),
        hex!("75104938baa47c54a86004ef998cc76c2e616289"),
        hex!("50028cf7ed245e4ac9e472d5277f14ed1c7ab384"),
    ];
    pub const SYSTEM_DENYLIST_INIT_DENIED_SENDERS: [Address; 6] = [
        hex!("78e044394595d4984f66c1b19059bc14ecc24063"),
        hex!("7b7ab20f75b691e90c546e89e41aa23b0a821444"),
        hex!("4489da9d81f0bc8125c8efdda1c117a7a895b43d"),
        hex!("3b647b46c9ba4fca221ccf933c09b653c2b4581f"),
        hex!("75104938baa47c54a86004ef998cc76c2e616289"),
        hex!("50028cf7ed245e4ac9e472d5277f14ed1c7ab384"),
    ];

    pub const SYSTEM_STAKING_GLOBAL_SETTINGS: GlobalPoolSettingsInfo = GlobalPoolSettingsInfo {
        min_pool_balance: l1x_sdk::types::U128(10000000000000000000000), // 10_000 tokens
        max_pool_balance: l1x_sdk::types::U128(5000000000000000000000000), // 5_000_000 tokens
        cool_down_period: l1x_sdk::types::U128(1209600000),              // 14 days
        max_pool_stakers: 1000,
    };
}
#[cfg(not(any(feature = "testnet", feature = "mainnet", feature = "devnet")))]
mod config_init_data {
    use primitives::Address;

    use crate::{config_init_mainnet_data, GlobalPoolSettingsInfo};

    pub const SYSTEM_CONFIG_INIT_DATA: &[u8] = config_init_mainnet_data::SYSTEM_CONFIG_INIT_DATA;
    pub const SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS: [Address; 6] =
        config_init_mainnet_data::SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS;
    pub const SYSTEM_DENYLIST_INIT_DENIED_SENDERS: [Address; 6] =
        config_init_mainnet_data::SYSTEM_DENYLIST_INIT_DENIED_SENDERS;

    pub const SYSTEM_STAKING_GLOBAL_SETTINGS: GlobalPoolSettingsInfo =
        config_init_mainnet_data::SYSTEM_STAKING_GLOBAL_SETTINGS;
}
#[cfg(any(feature = "mainnet"))]
mod config_init_data {
    use primitives::Address;

    use crate::config_init_mainnet_data;
    use crate::GlobalPoolSettingsInfo;

    pub const SYSTEM_CONFIG_INIT_DATA: &[u8] = config_init_mainnet_data::SYSTEM_CONFIG_INIT_DATA;
    pub const SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS: [Address; 6] =
        config_init_mainnet_data::SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS;
    pub const SYSTEM_DENYLIST_INIT_DENIED_SENDERS: [Address; 6] =
        config_init_mainnet_data::SYSTEM_DENYLIST_INIT_DENIED_SENDERS;

    pub const SYSTEM_STAKING_GLOBAL_SETTINGS: GlobalPoolSettingsInfo =
        config_init_mainnet_data::SYSTEM_STAKING_GLOBAL_SETTINGS;
}

#[cfg(feature = "testnet")]
mod config_init_data {
    use primitives::Address;

    use crate::GlobalPoolSettingsInfo;

    pub const SYSTEM_CONFIG_INIT_DATA: &[u8] = include_bytes!("../contracts_init_data/system_config_testnet.toml");
    pub const SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS: [Address; 0] = [];
    pub const SYSTEM_DENYLIST_INIT_DENIED_SENDERS: [Address; 0] = [];

    pub const SYSTEM_STAKING_GLOBAL_SETTINGS: GlobalPoolSettingsInfo = GlobalPoolSettingsInfo {
        max_pool_balance: l1x_sdk::types::U128(100_000),
        min_pool_balance: l1x_sdk::types::U128(10_000),
        cool_down_period: l1x_sdk::types::U128(1000 * 10), // 10 seconds
        max_pool_stakers: 1000,
    };
}
#[cfg(feature = "devnet")]
mod config_init_data {
    use primitives::Address;

    use crate::GlobalPoolSettingsInfo;

    pub const SYSTEM_CONFIG_INIT_DATA: &[u8] = include_bytes!("../contracts_init_data/system_config_devnet.toml");
    pub const SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS: [Address; 0] = [];
    pub const SYSTEM_DENYLIST_INIT_DENIED_SENDERS: [Address; 0] = [];

    pub const SYSTEM_STAKING_GLOBAL_SETTINGS: GlobalPoolSettingsInfo = GlobalPoolSettingsInfo {
        max_pool_balance: l1x_sdk::types::U128(100_000),
        min_pool_balance: l1x_sdk::types::U128(10_000),
        cool_down_period: l1x_sdk::types::U128(1000 * 10), // 10 seconds
        max_pool_stakers: 10_000,
    };
}

#[derive(Serialize)]
pub struct ConfigContractInitArgs {
    config_bytes: String,
    multisig_address: String,
}

impl ConfigContractInitArgs {
    pub fn new(multisig_address: &Address) -> Self {
        let multisig_address = hex::encode(multisig_address);
        let config_bytes = base64::engine::general_purpose::STANDARD.encode(config_init_data::SYSTEM_CONFIG_INIT_DATA);

        Self {
            config_bytes,
            multisig_address,
        }
    }
}

pub struct ConfigContractCallParams {
    pub function_name: Vec<u8>,
    pub args: Vec<u8>,
}

impl ConfigContractCallParams {
    pub fn config() -> Self {
        Self {
            function_name: "config".as_bytes().to_vec(),
            args: "{}".as_bytes().to_vec(),
        }
    }

    pub fn prev_config() -> Self {
        Self {
            function_name: "prev_config".as_bytes().to_vec(),
            args: "{}".as_bytes().to_vec(),
        }
    }
}

#[derive(Serialize)]
struct RoleBasedLists {
    pub receivers: BTreeSet<l1x_sdk::types::Address>,
    pub senders: BTreeSet<l1x_sdk::types::Address>,
}

#[derive(Serialize)]
struct RoleBasedListsOutput {
    deny_list: RoleBasedLists,
    allow_list: RoleBasedLists,
}

#[derive(Serialize)]
struct GeneralListsOutput {
    deny_list: BTreeSet<l1x_sdk::types::Address>,
    allow_list: BTreeSet<l1x_sdk::types::Address>,
}

#[derive(Serialize)]
pub struct DenyListContractInitArgs {
    name: String,
    multisig_address: String,
    initial_rb_lists: Option<RoleBasedListsOutput>,
    initial_general_lists: Option<GeneralListsOutput>,
}

impl DenyListContractInitArgs {
    pub fn new(name: String, multisig_address: &Address) -> Self {
        let multisig_address = hex::encode(multisig_address);

        let initial_rb_lists = if !config_init_data::SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS.is_empty()
            || !config_init_data::SYSTEM_DENYLIST_INIT_DENIED_SENDERS.is_empty()
        {
            Some(RoleBasedListsOutput {
                deny_list: RoleBasedLists {
                    receivers: BTreeSet::from_iter(
                        config_init_data::SYSTEM_DENYLIST_INIT_DENIED_RECEIVERS.iter().map(|v| v.into()),
                    ),
                    senders: BTreeSet::from_iter(
                        config_init_data::SYSTEM_DENYLIST_INIT_DENIED_SENDERS.iter().map(|v| v.into()),
                    ),
                },
                allow_list: RoleBasedLists {
                    receivers: BTreeSet::new(),
                    senders: BTreeSet::new(),
                },
            })
        } else {
            None
        };

        Self {
            name,
            multisig_address,
            initial_rb_lists,
            initial_general_lists: None,
        }
    }
}

pub struct DenyListContractCallParams {
    pub function_name: Vec<u8>,
    pub args: Vec<u8>,
}

impl DenyListContractCallParams {
    pub fn get_general_lists_borsh() -> Self {
        Self {
            function_name: "get_general_lists_borsh".as_bytes().to_vec(),
            args: "{}".as_bytes().to_vec(),
        }
    }

    pub fn get_rb_lists_borsh() -> Self {
        Self {
            function_name: "get_rb_lists_borsh".as_bytes().to_vec(),
            args: "{}".as_bytes().to_vec(),
        }
    }
}

#[derive(Serialize)]
pub struct MultisigContractInitArgs {
    pub approvers: BTreeSet<String>,
    pub allowed_proposers: BTreeSet<String>,
    pub majority: u16,
}

impl Default for MultisigContractInitArgs {
    fn default() -> Self {
        Self {
            approvers: BTreeSet::from_iter(MULTISIG_DEFAULT_APPROVERS.iter().map(|a| hex::encode(a))),
            allowed_proposers: BTreeSet::new(),
            majority: 51,
        }
    }
}

#[derive(Serialize)]
pub struct NodeRegistryContractInitArgs {
    pub multisig_address: l1x_sdk::types::Address,
    pub staking_pool_address: l1x_sdk::types::Address,
}

impl NodeRegistryContractInitArgs {
    pub fn new(multisig_address: &Address, staking_pool_address: &Address) -> Self {
        Self {
            multisig_address: multisig_address.into(),
            staking_pool_address: staking_pool_address.into(),
        }
    }
}

#[derive(Serialize)]
pub struct GlobalPoolSettingsInfo {
    pub max_pool_balance: l1x_sdk::types::U128,
    pub min_pool_balance: l1x_sdk::types::U128,
    pub cool_down_period: l1x_sdk::types::U128,
    pub max_pool_stakers: u32,
}

#[derive(Serialize)]
pub struct StakingPoolContractInitArgs {
    pub node_registry_address: l1x_sdk::types::Address,
    pub multisig_address: l1x_sdk::types::Address,
    pub global_settings: GlobalPoolSettingsInfo,
}

impl StakingPoolContractInitArgs {
    pub fn new(node_registry_address: &Address, multisig_address: &Address) -> Self {
        Self {
            node_registry_address: node_registry_address.into(),
            multisig_address: multisig_address.into(),
            global_settings: config_init_data::SYSTEM_STAKING_GLOBAL_SETTINGS,
        }
    }
}

pub struct StakingPoolCallParams {
    pub function_name: Vec<u8>,
    pub args: Vec<u8>,
}

impl StakingPoolCallParams {
    pub fn get_pool_info_for_nodes(nodes: Vec<Address>) -> Result<Self, serde_json::Error> {
        #[derive(Serialize)]
        struct Args {
            nodes: Vec<l1x_sdk::types::Address>,
        }

        let args = Args {
            nodes: nodes.iter().map(|n| n.into()).collect::<_>(),
        };

        Ok(Self {
            function_name: "get_pool_info_for_nodes".as_bytes().to_vec(),
            args: serde_json::to_vec(&args)?,
        })
    }

    pub fn get_pool_info_for_all_nodes() -> Self {
        Self {
            function_name: "get_pool_info_for_all_nodes".as_bytes().to_vec(),
            args: "{}".as_bytes().to_vec(),
        }
    }
}
