use anyhow::{anyhow, Error};
use evm::Capture;
use primitives::{Address, BlockHash, BlockNumber, BlockTimeStamp, Gas};
use state::UpdatedState;
use system::contract::ContractType;
use tokio::sync::broadcast;

use crate::{execute_common::ContractExecutor, execute_contract_evm, execute_contract_l1xvm, execute_contract_xtalk};

pub async fn smart_contract_read_only_call(
    contract_instance_address: &Address,
    function_name: &Vec<u8>,
    args: &Vec<u8>,
    gas_limit: Gas,
    cluster_address: &Address,
    block_number: BlockNumber,
    block_timestamp: BlockTimeStamp,
    block_hash: &BlockHash,
    updated_state: &mut UpdatedState) -> Result<Vec<u8>, Error> {
    // TODO: Future intern refactor - read only calls don't return events, but low level api's
    // require this event channel sender
    let (event_tx, _) = broadcast::channel(1000);

    tokio::task::block_in_place(|| {
        let contract_instance = updated_state.get_contract_instance(&contract_instance_address)?;
        let contract = updated_state.get_contract(&contract_instance.contract_address)?;
        let account = updated_state.get_account(&contract_instance.owner_address)?;
    
        let nonce = account.nonce + 1;
        let contract_type = contract.r#type.try_into()?;
        let executor: &(dyn ContractExecutor + Sync) = match contract_type {
            ContractType::L1XVM => &execute_contract_l1xvm::ExecuteContract,
            ContractType::XTALK => &execute_contract_xtalk::ExecuteContract,
            ContractType::EVM => &execute_contract_evm::ExecuteContract,
        };

        let current_call_depth = 0;
        let (result, _gas) = executor
            .execute_contract_function_call_read_only(
                &contract,
                &contract_instance,
                &function_name,
                &args,
                gas_limit,
                cluster_address,
                nonce,
                block_number,
                &block_hash,
                block_timestamp,
                current_call_depth,
                updated_state,
                event_tx,
            );

            match result {
                Capture::Exit(reason) => {
                    let (reason, result) = reason;
                    if reason.is_succeed() {
                        Ok((*result).clone())
                    } else {
                        Err(anyhow!("{}", hex::encode(result.as_ref())))
                    }
                },
                Capture::Trap(_resolve) => {
                    Err(anyhow!("Contract exited by Trap"))
                },
            }
    })
}