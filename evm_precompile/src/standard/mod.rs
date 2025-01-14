//! # Standard machines and gasometers
//!
//! This module implements the standard configurations of the interpreter, like how it works on
//! Ethereum mainnet. Most of them can still be customized to add additional functionality, by
//! wrapping them or replacing the generic parameters.

mod gasometer;
pub mod helper;
mod invoker;

pub use evm_runtime::Config;
/*pub use self::gasometer::{eval as eval_gasometer, GasometerState};
pub use self::invoker::{
	EtableResolver, Invoker, InvokerState, PrecompileSet, Resolver, TransactArgs,
};*/
pub use crate::standard::{
	gasometer::{eval as eval_gasometer, GasometerState},
	helper::MergeStrategy,
	invoker::{EtableResolver, Invoker, InvokerState, PrecompileSet, Resolver, TransactArgs},
};
use crate::{ExitError, RuntimeState};
use alloc::vec::Vec;
use evm_interpreter::GasState;
use primitive_types::{H160, H256, U256};

/// Standard machine.
pub type Machine<'config> = evm_interpreter::Machine<State<'config>>;

/// Standard Etable opcode handle function.
pub type Efn<'config, H> = evm_interpreter::Efn<State<'config>, H, evm_runtime::Opcode>;

/// Standard Etable.
pub type Etable<'config, H, F = Efn<'config, H>> =
	evm_interpreter::Etable<State<'config>, H, evm_runtime::Opcode, F>;

pub trait GasMutState: GasState {
	fn record_gas(&mut self, gas: U256) -> Result<(), ExitError>;
}

pub struct State<'config> {
	pub runtime: RuntimeState,
	pub gasometer: GasometerState<'config>,
}

impl<'config> AsRef<RuntimeState> for State<'config> {
	fn as_ref(&self) -> &RuntimeState {
		&self.runtime
	}
}

impl<'config> AsMut<RuntimeState> for State<'config> {
	fn as_mut(&mut self) -> &mut RuntimeState {
		&mut self.runtime
	}
}

impl<'config> AsRef<GasometerState<'config>> for State<'config> {
	fn as_ref(&self) -> &GasometerState<'config> {
		&self.gasometer
	}
}

impl<'config> AsMut<GasometerState<'config>> for State<'config> {
	fn as_mut(&mut self) -> &mut GasometerState<'config> {
		&mut self.gasometer
	}
}

impl<'config> GasState for State<'config> {
	fn gas(&self) -> U256 {
		self.gasometer.gas()
	}
}

impl<'config> GasMutState for State<'config> {
	fn record_gas(&mut self, gas: U256) -> Result<(), ExitError> {
		self.gasometer.record_gas(gas)
	}
}

impl<'config> InvokerState<'config> for State<'config> {
	fn new_transact_call(
		runtime: RuntimeState,
		gas_limit: U256,
		data: &[u8],
		access_list: &[(H160, Vec<H256>)],
		config: &'config Config,
	) -> Result<Self, ExitError> {
		Ok(Self {
			runtime,
			gasometer: GasometerState::new_transact_call(gas_limit, data, access_list, config)?,
		})
	}
	fn new_transact_create(
		runtime: RuntimeState,
		gas_limit: U256,
		code: &[u8],
		access_list: &[(H160, Vec<H256>)],
		config: &'config Config,
	) -> Result<Self, ExitError> {
		Ok(Self {
			runtime,
			gasometer: GasometerState::new_transact_create(gas_limit, code, access_list, config)?,
		})
	}

	fn substate(
		&mut self,
		runtime: RuntimeState,
		gas_limit: U256,
		is_static: bool,
		call_has_value: bool,
	) -> Result<Self, ExitError> {
		Ok(Self {
			runtime,
			gasometer: self.gasometer.submeter(gas_limit, is_static, call_has_value)?,
		})
	}
	fn merge(&mut self, substate: Self, strategy: MergeStrategy) {
		self.gasometer.merge(substate.gasometer, strategy)
	}

	fn record_codedeposit(&mut self, len: usize) -> Result<(), ExitError> {
		self.gasometer.record_codedeposit(len)
	}

	fn is_static(&self) -> bool {
		self.gasometer.is_static
	}
	fn effective_gas(&self) -> U256 {
		self.gasometer.effective_gas()
	}
	fn config(&self) -> &Config {
		self.gasometer.config
	}
}
