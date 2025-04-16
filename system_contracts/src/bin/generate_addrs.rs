use compile_time_config::SYSTEM_CONTRACTS_OWNER;
use hex_literal::hex;
use primitives::{Address, Nonce};
use sha3::{Digest, Keccak256};

// --- Define Network Cluster Addresses ---
// FIXME: Replace with actual addresses!
const MAINNET_CLUSTER_ADDRESS: Address = hex!("0101010101010101010101010101010101010101");
const TESTNET_CLUSTER_ADDRESS: Address = hex!("0404040404040404040404040404040404040404");
const DEVNET_CLUSTER_ADDRESS: Address = hex!("0303030303030303030303030303030303030303");

// --- Address Calculation Logic (copied for standalone use) ---

fn calculate_contract_address(
    account_address: &Address,
    cluster_address: &Address,
    nonce: Nonce,
) -> Address {
    let mut input: Vec<u8> = Vec::new();
    input.extend_from_slice(account_address);
    input.extend_from_slice(cluster_address);
    input.extend_from_slice(&nonce.to_be_bytes());

    let hash = Keccak256::digest(&input);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    address
}

fn calculate_contract_instance_address(
    account_address: &Address,
    contract_address: &Address,
    cluster_address: &Address,
    nonce: Nonce,
) -> Address {
    let mut input: Vec<u8> = Vec::new();
    input.extend_from_slice(account_address);
    input.extend_from_slice(contract_address);
    input.extend_from_slice(cluster_address);
    input.extend_from_slice(&nonce.to_be_bytes());

    let hash = Keccak256::digest(&input);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);
    address
}

// Helper to format Address for Rust code
fn format_address_const(addr: &Address) -> String {
    format!("hex!(\"{}\")", hex::encode(addr))
}

fn print_network_addresses(network_name: &str, cluster_address: &Address) {
    println!("// --- {} Addresses ({}) ---", network_name.to_uppercase(), hex::encode(cluster_address));

    let owner = &SYSTEM_CONTRACTS_OWNER;
    let mut nonce: Nonce = 0; 

    // Multisig
    nonce += 1; 
    let multisig_code_addr = calculate_contract_address(owner, cluster_address, nonce);
    nonce += 1;
    let multisig_instance_addr = calculate_contract_instance_address(owner, &multisig_code_addr, cluster_address, nonce);

    // Config
    nonce += 1;
    let config_code_addr = calculate_contract_address(owner, cluster_address, nonce);
    nonce += 1;
    let config_instance_addr = calculate_contract_instance_address(owner, &config_code_addr, cluster_address, nonce);

    // Node Registry
    nonce += 1;
    let node_registry_code_addr = calculate_contract_address(owner, cluster_address, nonce);
    nonce += 1;
    let node_registry_instance_addr = calculate_contract_instance_address(owner, &node_registry_code_addr, cluster_address, nonce);

    // Staking
    nonce += 1;
    let staking_code_addr = calculate_contract_address(owner, cluster_address, nonce);
    nonce += 1;
    let staking_instance_addr = calculate_contract_instance_address(owner, &staking_code_addr, cluster_address, nonce);

    // Denylist
    nonce += 1;
    let denylist_code_addr = calculate_contract_address(owner, cluster_address, nonce);
    nonce += 1;
    let denylist_instance_addr = calculate_contract_instance_address(owner, &denylist_code_addr, cluster_address, nonce);

    println!("pub const CLUSTER_ADDRESS: Address = {};", format_address_const(cluster_address));
    println!("pub const MULTISIG_CONTRACT_CODE_ADDRESS: Address = {};", format_address_const(&multisig_code_addr));
    println!("pub const MULTISIG_CONTRACT_INSTANCE_ADDRESS: Address = {};", format_address_const(&multisig_instance_addr));
    println!("pub const CONFIG_CONTRACT_CODE_ADDRESS: Address = {};", format_address_const(&config_code_addr));
    println!("pub const CONFIG_CONTRACT_INSTANCE_ADDRESS: Address = {};", format_address_const(&config_instance_addr));
    println!("pub const NODE_REGISTRY_CONTRACT_CODE_ADDRESS: Address = {};", format_address_const(&node_registry_code_addr));
    println!("pub const NODE_REGISTRY_CONTRACT_INSTANCE_ADDRESS: Address = {};", format_address_const(&node_registry_instance_addr));
    println!("pub const STAKING_CONTRACT_CODE_ADDRESS: Address = {};", format_address_const(&staking_code_addr));
    println!("pub const STAKING_CONTRACT_INSTANCE_ADDRESS: Address = {};", format_address_const(&staking_instance_addr));
    println!("pub const DENYLIST_CONTRACT_CODE_ADDRESS: Address = {};", format_address_const(&denylist_code_addr));
    println!("pub const DENYLIST_CONTRACT_INSTANCE_ADDRESS: Address = {};", format_address_const(&denylist_instance_addr));
    println!("");
}

fn main() {
    print_network_addresses("Mainnet", &MAINNET_CLUSTER_ADDRESS);
    print_network_addresses("Testnet", &TESTNET_CLUSTER_ADDRESS);
    print_network_addresses("Devnet", &DEVNET_CLUSTER_ADDRESS);
} 