//! Core layer for EVM.

// #![deny(warnings)]
// #![forbid(unsafe_code, unused_variables, unused_imports)]
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod call_create;
pub mod config;
pub mod error;
mod etable;
pub mod eval;
pub mod evmgasometer;
mod gasometer;
mod interpreter;
mod invoker;
mod memory;
pub mod opcode;
mod runtime;
mod stack;
pub mod utils;
mod valids;
pub use crate::{evmgasometer::*, gasometer::GasMutState, invoker::InvokerState};

pub use crate::{
	error::{Capture, ExitError, ExitException, ExitFatal, ExitResult, ExitSucceed},
	etable::{Control, Efn, Etable, EtableSet},
	interpreter::{EtableInterpreter, Interpreter, StepInterpreter},
	memory::Memory,
	opcode::Opcode,
	runtime::{
		CallCreateTrap, Context, GasState, Log, RuntimeBackend, RuntimeBaseBackend,
		RuntimeEnvironment, RuntimeState, TransactionContext, Transfer,
	},
	stack::Stack,
	valids::Valids,
};

use alloc::{rc::Rc, vec::Vec};

/// Merge strategy of a backend substate layer or a call stack gasometer layer.
#[derive(Clone, Debug, Copy)]
pub enum MergeStrategy {
	/// Fully commit the sub-layer into the parent. This happens if the sub-machine executes
	/// successfully.
	Commit,
	/// Revert the state, but keep remaining gases. This happens with the `REVERT` opcode.
	Revert,
	/// Discard the state and gases. This happens in all situations where the machine encounters an
	/// error.
	Discard,
}

/// Core execution layer for EVM.
pub struct Machine<S> {
	/// Program data.
	data: Rc<Vec<u8>>,
	/// Program code.
	code: Rc<Vec<u8>>,
	/// Return value. Note the difference between `retbuf`.
	/// A `retval` holds what's returned by the current machine, with `RETURN` or `REVERT` opcode.
	/// A `retbuf` holds the buffer of returned value by sub-calls.
	pub retval: Vec<u8>,
	/// Memory.
	pub memory: Memory,
	/// Stack.
	pub stack: Stack,
	/// Extra state,
	pub state: S,
}

impl<S> Machine<S> {
	/// Machine code.
	pub fn code(&self) -> &[u8] {
		&self.code
	}

	/// Create a new machine with given code and data.
	pub fn new(
		code: Rc<Vec<u8>>,
		data: Rc<Vec<u8>>,
		stack_limit: usize,
		memory_limit: usize,
		state: S,
	) -> Self {
		Self {
			data,
			code,
			retval: Vec::new(),
			memory: Memory::new(memory_limit),
			stack: Stack::new(stack_limit),
			state,
		}
	}

	/// Whether the machine has empty code.
	pub fn is_empty(&self) -> bool {
		self.code.is_empty()
	}
}
