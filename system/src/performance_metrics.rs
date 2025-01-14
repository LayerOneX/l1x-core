use std::collections::HashMap;
use primitives::Address;

pub struct PerformanceMetrics {
	block_production_rate: HashMap<Address, f64>,
	transaction_handling: HashMap<Address, f64>,
}

impl PerformanceMetrics {
	pub fn new() -> Self {
		PerformanceMetrics {
			block_production_rate: HashMap::new(),
			transaction_handling: HashMap::new(),
		}
	}

	pub fn update_block_production_rate(&mut self, validator_address: &Address, rate: f64) {
		self.block_production_rate.insert(validator_address.clone(), rate);
	}

	pub fn update_transaction_handling(&mut self, validator_address: &Address, rate: f64) {
		self.transaction_handling.insert(validator_address.clone(), rate);
	}

	pub fn get_block_production_rate(&self, validator_address: &Address) -> f64 {
		*self.block_production_rate.get(validator_address).unwrap_or(&0.0)
	}

	pub fn get_transaction_handling(&self, validator_address: &Address) -> f64 {
		*self.transaction_handling.get(validator_address).unwrap_or(&0.0)
	}
}
