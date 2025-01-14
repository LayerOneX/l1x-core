extern crate l1x_consensus_primitives;

type Gas = l1x_consensus_primitives::Gas;

pub struct RuntimeGas {
	pub gas_limit: Gas,
	pub burnt_gas: Gas,
}

impl RuntimeGas {
	pub fn new(gas_limit: Gas) -> Self {
		Self { gas_limit, burnt_gas: 0 }
	}

	pub fn new_unlimited() -> Self {
		Self::new(Gas::MAX)
	}

	pub fn burn(&mut self, gas: Gas) {
		self.burnt_gas = self.burnt_gas.checked_add(gas).unwrap_or(Gas::MAX);
	}

	pub fn left(&self) -> Gas {
		self.gas_limit.saturating_sub(self.burnt_gas)
	}
}
