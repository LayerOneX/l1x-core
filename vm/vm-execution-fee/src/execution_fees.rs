use primitives::*;
use rbpf::execution_fee::Fee;

#[derive(Debug, Clone, Copy)]
pub struct VmExecutionFees {
	pub vm_fee: Fee,
	pub cross_contract_call: Gas,
	// storage
	pub storage_write: Gas,
	pub storage_write_per_byte: Gas,
	pub storage_remove: Gas,
	// tokens
	pub token_transfer: Gas,
	// staking
	pub crate_staking_pool: Gas,
	pub stake: Gas,
	pub unstake: Gas,
	// events
	pub emit_event: Gas,
	pub emit_event_per_bytes: Gas,
}

impl VmExecutionFees {
	pub fn create_basic_config() -> Self {
		Self {
			vm_fee: Fee::create_basic_config(),
			cross_contract_call: 1,
			storage_write: 1,
			storage_write_per_byte: 0,
			storage_remove: 1,
			token_transfer: 1,
			crate_staking_pool: 1,
			stake: 1,
			unstake: 1,
			emit_event: 1,
			emit_event_per_bytes: 0,
		}
	}
}
