use primitives::{Balance, Gas};

/// Dummy Gas Station
pub struct GasStation {
	gas_price: Balance,
}

impl GasStation {
	pub fn from_fixed_price(gas_price: Balance, _evm_gas_price: Balance) -> Self {
		Self { gas_price }
	}

	pub fn buy_gas(&self, balance: Balance) -> Gas {
		let gas = balance.checked_mul(self.gas_price).unwrap_or(Gas::MAX.into());

		Gas::try_from(gas).unwrap_or(Gas::MAX)
	}

	pub fn sell_gas(&self, gas: Gas) -> Balance {
		if gas == 0 {
			0
		} else {
			let res = gas.checked_div(self.gas_price as _).unwrap_or(0) + 1;
			res as Balance
		}
	}

	pub fn gas_price(&self) -> Balance {
		self.gas_price
	}
}

#[cfg(test)]
mod tests {
    use primitives::Gas;

    use super::GasStation;

    #[test]
    pub fn test_buy_gas() {
        let l1x_gas_price = 1000;
        let evm_gas_price = 1000;
        let gas_station = GasStation::from_fixed_price(l1x_gas_price, evm_gas_price);
		
		assert_eq!(gas_station.buy_gas(0), 0);
		assert_eq!(gas_station.buy_gas(1), l1x_gas_price as Gas);

		let zero_price = 0;
		let gas_station = GasStation::from_fixed_price(zero_price, zero_price);
		assert_eq!(gas_station.buy_gas(0), 0);
		assert_eq!(gas_station.buy_gas(1), 0);
    }

	#[test]
	pub fn test_sell_gas() {
        let l1x_gas_price = 1000;
        let evm_gas_price = 1000;
        let gas_station = GasStation::from_fixed_price(l1x_gas_price, evm_gas_price);
		
		assert_eq!(gas_station.sell_gas(0), 0);
		assert_eq!(gas_station.sell_gas(1), 1);

		let zero_price = 0;
		let gas_station = GasStation::from_fixed_price(zero_price, zero_price);
		assert_eq!(gas_station.sell_gas(0), 0);
		assert_eq!(gas_station.sell_gas(1), 1);
		assert_eq!(gas_station.sell_gas(l1x_gas_price as Gas), 1);
    }

	#[test]
	pub fn test_buy_sell() {
		let l1x_gas_price = 1000;
        let evm_gas_price = 1000;
        let gas_station = GasStation::from_fixed_price(l1x_gas_price, evm_gas_price);

		let balance = 666;
		let gas = gas_station.buy_gas(balance) - 1;
		assert_eq!(balance, gas_station.sell_gas(gas));
	}
}