use std::fmt;

use primitives::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct StakingAccount {
	pub account_address: Address,
	pub pool_address: Address,
	pub balance: Balance,
}

impl StakingAccount {
	pub fn new(account_address: Address, pool_address: Address) -> StakingAccount {
		StakingAccount { account_address, pool_address, balance: 0 }
	}
}

impl fmt::Display for StakingAccount {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"StakingAccount {{ account_address: 0x{}, pool_address: 0x{}, balance: {} L1X Tokens }}",
			hex::encode(self.account_address), hex::encode(self.pool_address), self.balance,
		)
	}
}
