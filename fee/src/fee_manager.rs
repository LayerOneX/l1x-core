use anyhow::{anyhow, Error};
use compile_time_config::{EVM_GAS_PRICE, GAS_PRICE};
use primitives::{Balance, Gas};
use crate::gas_station::GasStation;

// Keeps track of total fee used, gas used and available fee limit
pub struct FeeManager {
    fee_limit: Balance,
    gas_used: Gas,
    left_limit: Balance,
    gas_station: GasStation,
}

impl FeeManager {
    pub fn new(fee_limit: Balance) -> anyhow::Result<Self> {
        Ok(FeeManager {
            fee_limit,
            gas_used: 0,
            left_limit: fee_limit,
            gas_station: GasStation::from_fixed_price(
                GAS_PRICE,
                EVM_GAS_PRICE,
            ),
        })
    }

    pub fn update_fee(&mut self, fee: Balance) -> Result<(), Error> {
        self.left_limit = match self.left_limit.checked_sub(fee) {
            Some(fee) => fee,
            None => {
                self.left_limit = 0;
                return Err(anyhow!("Update fee: Insufficient fee limit"))
            }
        };
        Ok(())
    }

    pub fn get_gas_limit(&self) -> Result<Gas, Error> {
        Ok(self.gas_station.buy_gas(self.left_limit))
    }

    pub fn update_gas_and_fee_used(&mut self, burnt_gas: Gas) -> Result<(), Error> {
        let fee_used = self.gas_station.sell_gas(burnt_gas);
        self.left_limit = match self.left_limit.checked_sub(fee_used) {
            Some(fee) => fee,
            None => {
                self.gas_used = self.gas_used.checked_add(burnt_gas).unwrap_or(Gas::MAX);
                self.left_limit = 0;
                return Err(anyhow!("Update gas & fee: Insufficient fee limit"))
            }
        };
        self.gas_used = self.gas_used.checked_add(burnt_gas).unwrap_or(Gas::MAX);
        Ok(())
    }

    pub fn get_fee_used(&self) -> Balance {
        self.fee_limit - self.left_limit
    }

    pub fn get_gas_used(&self) -> Gas {
        self.gas_used
    }
}