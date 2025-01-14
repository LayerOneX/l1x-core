use account::account_state::{self};
use anyhow::{Error, anyhow};
use primitives::{Address, ContractInstanceKey, ContractInstanceValue};
use system::{account::Account, contract::Contract, contract_instance::ContractInstance, staking_account::StakingAccount, staking_pool::StakingPool};
use log::error;
use tokio::sync::mpsc;
use tokio_util::sync::{CancellationToken, DropGuard};

macro_rules! send_result {
    ($expr:expr, $response:expr) => {
        match $expr {
            Ok(val) => {
                if let Err(e) = $response.send(Ok(val)) {
                    error!("DB: Couldn't send a response to the channel, error: {:?}", e);
                }
            },
            Err(e) => {
                if let Err(e) = $response.send(Err(e)) {
                    error!("DB: Couldn't send a response to the channel, error: {:?}", e);
                }
            },
        }
    };
}

macro_rules! exit_on_error {
    ($expr:expr, $response:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => {
                if let Err(e) = $response.send(Err(e)) {
                    error!("DB: Couldn't send an error message to the channel, error: {:?}", e);
                }
                return
            },
        }
    };
}

#[derive(Debug)]
pub enum DBRequest {
	GetAccount {
		address: Address,
		response: tokio::sync::oneshot::Sender<Result<Account, Error>>,
	},
	GetContract {
		address: Address,
		response: tokio::sync::oneshot::Sender<Result<Contract, Error>>,
	},
	GetContractInstance {
		address: Address,
		response: tokio::sync::oneshot::Sender<Result<ContractInstance, Error>>,
	},
	GetContractInstanceValue {
		address: Address,
		key: ContractInstanceKey,
		response: tokio::sync::oneshot::Sender<Result<Option<ContractInstanceValue>, Error>>,
	},
    GetStakingPool {
        address: Address,
		response: tokio::sync::oneshot::Sender<Result<StakingPool, Error>>,
    },
    GetStakingAccount {
        pool_address: Address,
        account_address: Address,
		response: tokio::sync::oneshot::Sender<Result<StakingAccount, Error>>,
    }
}

pub async fn run_db_handler<'a>() -> Result<(mpsc::Sender<DBRequest>, DropGuard), Error> {
    let (tx, rx) = tokio::sync::mpsc::channel(1000);
    let (status_tx, status_rx) = tokio::sync::oneshot::channel();
    let cancel_token = tokio_util::sync::CancellationToken::new();
    tokio::task::spawn(db_request_handler(status_tx, rx, cancel_token.clone()));

    match status_rx.await {
        Ok(init_status) => match init_status {
            Ok(_) => Ok((tx, cancel_token.drop_guard())),
            Err(e) => Err(anyhow!("Can't initialize db_request_handler: {}", e))
        },
        Err(e) => {
            Err(anyhow!("Can't receive db_request_handler status: {}", e))
        }
    }
}

async fn db_request_handler(status_tx: tokio::sync::oneshot::Sender<Result<(), Error>>, mut rx: tokio::sync::mpsc::Receiver<DBRequest>, cancelation_token: CancellationToken) {
    let db_pool_conn = exit_on_error!(db::db::Database::get_pool_connection().await, status_tx);
    let account_state = exit_on_error!(account_state::AccountState::new(&db_pool_conn).await, status_tx);
    let contract_state = exit_on_error!(contract::contract_state::ContractState::new(&db_pool_conn).await, status_tx);
    let contract_instance_state = exit_on_error!(contract_instance::contract_instance_state::ContractInstanceState::new(&db_pool_conn).await, status_tx);
    let staking_state = exit_on_error!(staking::staking_state::StakingState::new(&db_pool_conn).await, status_tx);

    if let Err(_) = status_tx.send(Ok(())) {
        error!("DB: Can't send the initialization status. Sender is dropped");
        return;
    }

    loop {
        tokio::select! {
            _ = cancelation_token.cancelled() => {
                break;
            },
            msg = rx.recv() => {
                match msg {
                    Some(request) => match request {
                        DBRequest::GetAccount { address, response } => {
                            send_result!(account_state.get_account(&address).await, response);
                        },
                        DBRequest::GetContract { address, response } => {
                                send_result!(contract_state.get_contract(&address).await, response);
                        },
                        DBRequest::GetContractInstance { address, response } => {
                                send_result!(contract_instance_state.get_contract_instance(&address).await, response);
                        },
                        DBRequest::GetContractInstanceValue { address, key, response } => {
                                send_result!(contract_instance_state.get_state_key_value(&address, &key).await, response);
                        },
                        DBRequest::GetStakingPool { address, response } => {
                                send_result!(staking_state.get_staking_pool(&address).await, response);
                        },
                        DBRequest::GetStakingAccount { pool_address, account_address, response } => {
                                send_result!(staking_state.get_staking_account(&account_address, &pool_address).await, response);
                        },
                    },
                    None => {
                        break;
                    }
                }
            }
        };
        tokio::task::yield_now().await;
    }
}