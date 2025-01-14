use primitives::{Balance, Gas};

pub const MAX_CROSS_CONTRACT_CALL_DEPTH: u32 = 15;
pub const READONLY_CALL_DEFAULT_GAS_LIMIT: Gas = 30_000_000; // 10_000_000 is burnt for ~1.4 secs on my laptop
pub const READONLY_SYSTEM_CALL_GAS_LIMIT: Gas = Gas::MAX;
pub const ESTIMATE_FEE_LIMIT: Balance = 10_000_000;
pub const ETH_CALL_FEE_LIMIT: Balance = 10_000_000;
