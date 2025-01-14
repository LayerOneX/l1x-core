use borsh::BorshDeserialize;
use borsh::BorshSerialize;
use l1x_sdk::call_contract;
use l1x_sdk::contract;
use l1x_sdk::contract_interaction::ContractCall;
use l1x_sdk::emit_event_experimental;
use l1x_sdk::types::Address;

use l1x_sdk::types::U64;

const STORAGE_CONTRACT_KEY: &[u8] = b"STATE";

#[derive(BorshSerialize)]
struct Event {
    name: String,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct Contract {
    counter: U64,
}

#[contract]
impl Contract {
    fn load() -> Self {
        match l1x_sdk::storage_read(STORAGE_CONTRACT_KEY) {
            Some(bytes) => Self::try_from_slice(&bytes).unwrap(),
            None => panic!("The contract isn't initialized"),
        }
    }

    fn save(&mut self) {
        l1x_sdk::storage_write(STORAGE_CONTRACT_KEY, &self.try_to_vec().unwrap());
    }

    pub fn new() {
        let mut state = Self {
            counter: 0u64.into(),
        };

        state.save()
    }

    pub fn cross_contract_call(
        address: Address,
        method_name: String,
        args: String,
        read_only: bool,
        gas_limit: U64,
    ) {
        let call = ContractCall {
            contract_address: address.clone(),
            method_name,
            args: args.into_bytes(),
            read_only,
            gas_limit: gas_limit.0,
        };

        match call_contract(&call) {
            Ok(res) => {
                let res: Result<Vec<String>, serde_json::Error> = serde_json::from_slice(&res);
                l1x_sdk::msg(&format!("Returned by the external contract: {:?}", res));
            }
            Err(e) => {
                panic!("call_contract failed: {e}")
            }
        }
    }

    pub fn set_counter(value: U64) -> U64 {
        let mut state = Self::load();
        let old = state.counter;
        state.counter = value;
        state.save();

        old
    }

    pub fn inc_counter() -> U64 {
        let mut state = Self::load();
        let old = state.counter;
        state.counter.0 += 1;
        state.save();

        old
    }

    pub fn get_counter() -> U64 {
        Self::load().counter
    }

    pub fn infinite_loop() {
        loop {}
    }

    pub fn emit_event() {
        emit_event_experimental(Event {
            name: "Hello".to_string(),
        });
    }
}
