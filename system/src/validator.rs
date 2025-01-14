use std::fmt;
use primitives::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Validator {
	pub address: Address,
	pub cluster_address: Address,
	pub epoch: Epoch,
	pub stake: Balance,
	pub xscore: f64,
}

impl Validator {
	pub fn new(
		address: Address,
		cluster_address: Address,
		epoch: Epoch,
		stake: Balance,
		xscore: f64,
	) -> Validator {
		Validator { address, cluster_address, epoch, stake, xscore }
	}
	
}

impl fmt::Display for Validator {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"Validator {{ address: 0x{}, cluster_address: 0x{}, epoch #{}, stake: {} L1X tokens, xscore: {} }}",
			hex::encode(self.address),
			hex::encode(self.cluster_address),
			self.epoch,
			self.stake,
			self.xscore
		)
	}
}

impl fmt::Debug for Validator {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Display::fmt(&self, f)
	}
}