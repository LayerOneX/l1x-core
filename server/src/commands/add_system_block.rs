use std::{fs::read_to_string, path::PathBuf};
use account::account_state::AccountState;
use anyhow::{Error, anyhow};
use block::{block_manager::BlockManager, block_state::BlockState};
use compile_time_config::SYSTEM_CONTRACTS_OWNER;
use contract::contract_state::ContractState;
use contract_instance::contract_instance_state::ContractInstanceState;
use db::db::Database;
use directories::UserDirs;
use primitives::{Address, Balance, ContractCode};
use structopt::StructOpt;
use system::{
    account::Account,
    config::Config,
    transaction::{Transaction, TransactionType},
};
use log::{info, error};
use system_contracts::{
    CONFIG_CONTRACT_CODE_ADDRESS, CONFIG_CONTRACT_INSTANCE_ADDRESS, DENYLIST_CONTRACT_CODE_ADDRESS, DENYLIST_CONTRACT_INSTANCE_ADDRESS,
    MULTISIG_CONTRACT_CODE_ADDRESS, MULTISIG_CONTRACT_INSTANCE_ADDRESS, NODE_REGISTRY_CONTRACT_CODE_ADDRESS,
    NODE_REGISTRY_CONTRACT_INSTANCE_ADDRESS, STAKING_CONTRACT_CODE_ADDRESS, STAKING_CONTRACT_INSTANCE_ADDRESS,
};

#[derive(Debug, StructOpt)]
#[structopt(name = "add-system-block")]
pub struct AddSystemBlockCmd {
	#[structopt(long = "path", short = "w")]
	working_dir: Option<PathBuf>,
}

impl AddSystemBlockCmd {
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
        match read_to_string(&config_path) {
            Ok(contents) => match toml::from_str::<Config>(&contents) {
                Ok(config) => {
                    parsed_config = config;
                }
                Err(e) => println!("Could not parse config.toml: {:?}", e),
            },
            Err(e) => println!("Could not read config.toml: {:?}", e),
        }

		let cluster_address: Address = hex::decode(&parsed_config.cluster_address)
			.expect("unable to decode cluster address")
			.try_into()
			.expect("Wrong length of Vec");

		if !parsed_config.boot_nodes.is_empty() {
			error!("Boot nodes are not empty. Only a root node can create system blocks");
			return;
		}

        Database::new(&parsed_config).await;

		let db_pool_conn =
			Database::get_pool_connection().await.expect("unable to get db_pool_conn");

		if let Err(e) = add_system_block(&cluster_address, &db_pool_conn).await {
			panic!("Can't add a system block, error: {}", e);
		}
	}
}

enum TransactionInfo {
	SystemContractDeploymentInfo(SystemContractDeploymentInfo),
	SystemContractInitInfo(SystemContractInitInfo)
}

impl From<SystemContractDeploymentInfo> for TransactionInfo {
	fn from(value: SystemContractDeploymentInfo) -> Self {
		Self::SystemContractDeploymentInfo(value)
	}
}

impl From<SystemContractInitInfo> for TransactionInfo {
	fn from(value: SystemContractInitInfo) -> Self {
		Self::SystemContractInitInfo(value)
	}
}

struct SystemContractDeploymentInfo {
	name: String,
	contract_deployment_tx: Transaction,
	contract_initialization_tx: Transaction,
	contract_code_address: Address,
	contract_instance_address: Address,
}

struct SystemContractInitInfo {
	name: String,
	contract_initialization_tx: Transaction,
	contract_instance_address: Address,
}

fn generate_deploy_transactions(name: String, system_account: &mut Account, cluster_address: &Address, contract_code: ContractCode, arguments: Vec<u8>, fee_limit: Balance) -> SystemContractDeploymentInfo {
	let tx_type = TransactionType::SmartContractDeployment {
		access_type: system::access::AccessType::PUBLIC,
		contract_type: system::contract::ContractType::L1XVM,
		contract_code,
		deposit: 0,
		salt: [0; 32].to_vec(),
	};
	system_account.nonce += 1;
	let contract_deployment_tx = Transaction::new_system(&system_account.address, system_account.nonce, tx_type, fee_limit);

	let contract_code_address = Account::contract_address(&system_account.address, &cluster_address, system_account.nonce);

	let init_tx_info = generate_smart_contract_init_transaction(name.clone(), system_account, cluster_address, &contract_code_address, arguments, fee_limit);

	SystemContractDeploymentInfo {
		name,
		contract_deployment_tx,
		contract_initialization_tx: init_tx_info.contract_initialization_tx,
		contract_code_address,
		contract_instance_address: init_tx_info.contract_instance_address,
	}
	
}

fn generate_smart_contract_init_transaction(name: String, system_account: &mut Account, cluster_address: &Address, contract_code_address: &Address, arguments: Vec<u8>, fee_limit: Balance) -> SystemContractInitInfo {
	let tx_type = TransactionType::SmartContractInit {
		contract_code_address: *contract_code_address,
		arguments,
		deposit: 0
	};
	system_account.nonce += 1;
	let contract_initialization_tx = Transaction::new_system(&system_account.address, system_account.nonce, tx_type, fee_limit);

	let contract_instance_address = Account::contract_instance_address(&system_account.address, &contract_code_address, &cluster_address, system_account.nonce);

	SystemContractInitInfo {
		name,
		contract_initialization_tx,
		contract_instance_address,
	}
}

async fn contract_is_initialized<'a>(contract_code_address: &Address, contract_instance_address: &Address, db_pool_conn: &'a db::db::DbTxConn<'a>) -> Result<(), Error> {
	let contract_state = ContractState::new(db_pool_conn).await?;
	let contract_instance_state = ContractInstanceState::new(db_pool_conn).await?;

	if !contract_state.is_valid_contract(contract_code_address).await? {
		Err(anyhow!("Contract code address is invalid"))
	} else if !contract_instance_state.is_valid_contract_instance(contract_instance_address).await? {
		Err(anyhow!("Contract instance address is invalid"))
	} else {
		Ok(())
	}
}

async fn add_system_block<'a>(cluster_address: &Address, db_pool_conn: &'a db::db::DbTxConn<'a>) -> Result<(), Error> {
	let account_state = AccountState::new(&db_pool_conn).await?;
	
	let mut system_account = Account::new_system(compile_time_config::SYSTEM_CONTRACTS_OWNER);
	match account_state.is_valid_account(&system_account.address).await {
		Ok(true) => (),
		Ok(false)
		| Err(_) => {
			account_state.create_account(&system_account).await?;
		}
	}

    let multisig_is_initialized = contract_is_initialized(
        &MULTISIG_CONTRACT_CODE_ADDRESS,
        &MULTISIG_CONTRACT_INSTANCE_ADDRESS,
        db_pool_conn,
    )
    .await
    .is_ok();
    let config_is_initialized = contract_is_initialized(
        &CONFIG_CONTRACT_CODE_ADDRESS,
        &CONFIG_CONTRACT_INSTANCE_ADDRESS,
        db_pool_conn,
    )
    .await
    .is_ok();
    let node_registry_is_intialized = contract_is_initialized(
        &NODE_REGISTRY_CONTRACT_CODE_ADDRESS,
        &NODE_REGISTRY_CONTRACT_INSTANCE_ADDRESS,
        db_pool_conn,
    )
    .await
    .is_ok();
    let staking_is_initialized = contract_is_initialized(
        &STAKING_CONTRACT_CODE_ADDRESS,
        &STAKING_CONTRACT_INSTANCE_ADDRESS,
        db_pool_conn,
    )
    .await
    .is_ok();
    let deny_config_is_initialized = contract_is_initialized(
        &DENYLIST_CONTRACT_CODE_ADDRESS,
        &DENYLIST_CONTRACT_INSTANCE_ADDRESS,
        db_pool_conn,
    )
    .await
    .is_ok();

    if multisig_is_initialized && config_is_initialized && node_registry_is_intialized && staking_is_initialized{
		info!("All contracts are already initialized");
		return Ok(())
	}

	let block_manager = BlockManager::new();

	let mut transaction_infos: Vec<TransactionInfo> = Vec::new();

	// WARNING: The sequence of the transactions is important because `nonce`` is incremented
	// after each transaction but the address genration depends on `nonce``.
	if !multisig_is_initialized {
		let info = generate_deploy_transactions(
			"multisig".to_owned(),
			&mut system_account,
			&cluster_address,
			system_contracts::MULTISIG_CONTRACT_OBJECT_BYTES.to_vec(),
			serde_json::to_vec(&system_contracts::MultisigContractInitArgs::default())?,
			1_000_000_000
		);
	
		if info.contract_code_address != MULTISIG_CONTRACT_CODE_ADDRESS {
			return Err(anyhow!("Multisig contract code address mismatch"));
		}
		if info.contract_instance_address != MULTISIG_CONTRACT_INSTANCE_ADDRESS {
			return Err(anyhow!("Multisig contract instance address mismatch"));
		}

		transaction_infos.push(info.into());
	}
    if !config_is_initialized {
        let info = generate_deploy_transactions(
            "config".to_owned(),
            &mut system_account,
            &cluster_address,
            system_contracts::CONFIG_CONTRACT_OBJECT_BYTES.to_vec(),
            serde_json::to_vec(&system_contracts::ConfigContractInitArgs::new(
                &MULTISIG_CONTRACT_INSTANCE_ADDRESS,
            ))?,
            1_000_000_000,
        );
        if info.contract_code_address != CONFIG_CONTRACT_CODE_ADDRESS {
            return Err(anyhow!("Config contract code address mismatch"));
        }
        if info.contract_instance_address != CONFIG_CONTRACT_INSTANCE_ADDRESS {
            return Err(anyhow!("Config contract instance address mismatch"));
        }
        transaction_infos.push(info.into());
    }

    if !node_registry_is_intialized {
        let info = generate_deploy_transactions(
            "node_registry".to_owned(),
            &mut system_account,
            &cluster_address,
            system_contracts::NODE_REGISTRY_CONTRACT_OBJECT_BYTES.to_vec(),
            serde_json::to_vec(&system_contracts::NodeRegistryContractInitArgs::new(
                &MULTISIG_CONTRACT_INSTANCE_ADDRESS,
                &STAKING_CONTRACT_INSTANCE_ADDRESS,
            ))?,
            1_000_000_000,
        );
        if info.contract_code_address != NODE_REGISTRY_CONTRACT_CODE_ADDRESS {
            return Err(anyhow!("Node Registry contract code address mismatch"));
        }
        if info.contract_instance_address != NODE_REGISTRY_CONTRACT_INSTANCE_ADDRESS {
            return Err(anyhow!("Node Registry contract instance address mismatch"));
        }
        transaction_infos.push(info.into());
    }
    if !staking_is_initialized {
        let info = generate_deploy_transactions(
            "staking".to_owned(),
            &mut system_account,
            &cluster_address,
            system_contracts::STAKING_CONTRACT_OBJECT_BYTES.to_vec(),
            serde_json::to_vec(&system_contracts::StakingPoolContractInitArgs::new(
                &NODE_REGISTRY_CONTRACT_INSTANCE_ADDRESS,
                &MULTISIG_CONTRACT_INSTANCE_ADDRESS,
            ))?,
            1_000_000_000,
        );
        if info.contract_code_address != STAKING_CONTRACT_CODE_ADDRESS {
            return Err(anyhow!(
                "Staking contract code address mismatch: {}",
                hex::encode(&info.contract_code_address)
            ));
        }
        if info.contract_instance_address != STAKING_CONTRACT_INSTANCE_ADDRESS {
            return Err(anyhow!(
                "Staking contract instance address mismatch: {}",
                hex::encode(&info.contract_instance_address)
            ));
        }
        transaction_infos.push(info.into());
    }
    if !deny_config_is_initialized {
        let info = generate_deploy_transactions(
            "denylist".to_owned(),
            &mut system_account,
            &cluster_address,
            system_contracts::DENYLIST_CONTRACT_OBJECT_BYTES.to_vec(),
            serde_json::to_vec(&system_contracts::DenyListContractInitArgs::new(
                "denylist".to_string(),
                &MULTISIG_CONTRACT_INSTANCE_ADDRESS,
            ))?,
            1_000_000_000,
        );
        if info.contract_code_address != DENYLIST_CONTRACT_CODE_ADDRESS {
            return Err(anyhow!(
                "Denylist contract code address mismatch: {}",
                hex::encode(&info.contract_code_address)
            ));
        }
        if info.contract_instance_address != DENYLIST_CONTRACT_INSTANCE_ADDRESS {
            return Err(anyhow!(
                "Denylist contract instance address mismatch: {}",
                hex::encode(&info.contract_instance_address)
            ));
        }
        transaction_infos.push(info.into());
    }
    
    let mut transactions = Vec::new();
    transaction_infos.iter().for_each(|info| {
        match info {
            TransactionInfo::SystemContractDeploymentInfo(info) => {
                transactions.push(info.contract_deployment_tx.clone());
                transactions.push(info.contract_initialization_tx.clone());
            },
            TransactionInfo::SystemContractInitInfo(info) => {
                transactions.push(info.contract_initialization_tx.clone());
            }
        }
    });

    let block_state = BlockState::new(&db_pool_conn).await?;
    let block = block_manager.create_system_block(transactions, *cluster_address, &block_state, &account_state).await?;
	let block_number = block.block_header.block_number;
	let block_hash = block.block_header.block_hash;

    block_state.store_block(block.clone()).await?;

    let (event_tx, _) = tokio::sync::broadcast::channel(1000);

    execute::execute_block::ExecuteBlock::execute_block(&block, event_tx, &db_pool_conn).await?;

    for info in &transaction_infos {
        match info {
            TransactionInfo::SystemContractDeploymentInfo(info) => {
                if !account_state.is_valid_account(&info.contract_code_address).await? {
                    return Err(anyhow!(
                        "Could not deploy the system contract '{}' to address {}",
                        info.name,
                        hex::encode(&info.contract_code_address)
                    ));
                }
                info!(
                    "'{}' system contract code is deployed to address {}",
                    info.name,
                    hex::encode(&info.contract_code_address)
                );
                if !account_state.is_valid_account(&info.contract_instance_address).await? {
                    return Err(anyhow!(
                        "Could initialize the system contract '{}' to address {}",
                        info.name,
                        hex::encode(&info.contract_instance_address)
                    ));
                }
                info!(
                    "'{}' system contract is intialized to address {}",
                    info.name,
                    hex::encode(&info.contract_instance_address)
                );
            }
            TransactionInfo::SystemContractInitInfo(info) => {
                if !account_state.is_valid_account(&info.contract_instance_address).await? {
                    return Err(anyhow!(
                        "Could initialize the system contract '{}' to address {}",
                        info.name,
                        hex::encode(&info.contract_instance_address)
                    ));
                }
                info!(
                    "'{}' system contract is intialized to address {}",
                    info.name,
                    hex::encode(&info.contract_instance_address)
                );
            }
        }
    }

    let new_nonce = account_state.get_nonce(&SYSTEM_CONTRACTS_OWNER).await?;
    let expected_nonce = transaction_infos.iter().map(|v| {
        match v {
            TransactionInfo::SystemContractDeploymentInfo(_) => 2,
            TransactionInfo::SystemContractInitInfo(_) => 1,
        }
    }).sum();

    if new_nonce != expected_nonce {
        return Err(anyhow!(
            "System Contract Owner nonce is not equal to the expected one, expected: {}, actual: {}",
            expected_nonce,
            new_nonce
        ));
    }

    info!(
        "System Block #{}, {} has been added and executed",
        block_number,
        hex::encode(&block_hash)
    );
    Ok(())
}
