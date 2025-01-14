use crate::standard::invoker::invoker::InvokerControl;
use alloc::{rc::Rc, vec::Vec};
use core::marker::PhantomData;
use evm_interpreter::{
	EtableInterpreter, EtableSet, ExitError, ExitResult, Interpreter, Machine, RuntimeBackend,
	RuntimeState,
};
use evm_runtime::Config;
use primitive_types::H160;

/// A code resolver.
///
/// The resolver handles how a call (with the target code address) or create
/// (with the init code) is turned into a colored machine. The resolver can
/// construct a machine, pushing the call stack, or directly exit, handling a
/// precompile.
pub trait Resolver<S, H, Tr> {
	type Interpreter: Interpreter<S, H, Tr>;

	/// Resolve a call (with the target code address).
	#[allow(clippy::type_complexity)]
	fn resolve_call(
		&self,
		code_address: H160,
		input: Vec<u8>,
		state: S,
		handler: &mut H,
	) -> Result<InvokerControl<Self::Interpreter, (ExitResult, (S, Vec<u8>))>, ExitError>;

	/// Resolve a create (with the init code).
	#[allow(clippy::type_complexity)]
	fn resolve_create(
		&self,
		init_code: Vec<u8>,
		state: S,
		handler: &mut H,
	) -> Result<InvokerControl<Self::Interpreter, (ExitResult, (S, Vec<u8>))>, ExitError>;
}

/// A set of precompiles.
pub trait PrecompileSet<S, H> {
	/// Attempt to execute the precompile at the given `code_address`. Returns
	/// `None` if it's not a precompile.
	fn execute(
		&self,
		code_address: H160,
		input: &[u8],
		state: &mut S,
		handler: &mut H,
	) -> Option<(ExitResult, Vec<u8>)>;
}

impl<S, H> PrecompileSet<S, H> for () {
	fn execute(
		&self,
		_code_address: H160,
		_input: &[u8],
		_state: &mut S,
		_handler: &mut H,
	) -> Option<(ExitResult, Vec<u8>)> {
		None
	}
}

/// The standard code resolver where the color is an [Etable]. This is usually
/// what you need.
pub struct EtableResolver<'config, 'precompile, 'etable, S, H, Pre, Tr, ES> {
	config: &'config Config,
	etable: &'etable ES,
	precompiles: &'precompile Pre,
	_marker: PhantomData<(S, H, Tr)>,
}

impl<'config, 'precompile, 'etable, S, H, Pre, Tr, ES>
	EtableResolver<'config, 'precompile, 'etable, S, H, Pre, Tr, ES>
{
	pub fn new(
		config: &'config Config,
		precompiles: &'precompile Pre,
		etable: &'etable ES,
	) -> Self {
		Self { config, precompiles, etable, _marker: PhantomData }
	}
}

impl<'config, 'precompile, 'etable, S, H, Pre, Tr, ES> Resolver<S, H, Tr>
	for EtableResolver<'config, 'precompile, 'etable, S, H, Pre, Tr, ES>
where
	S: AsRef<RuntimeState> + AsMut<RuntimeState>,
	H: RuntimeBackend,
	Pre: PrecompileSet<S, H>,
	ES: EtableSet<S, H, Tr>,
{
	type Interpreter = EtableInterpreter<'etable, S, H, Tr, ES>;

	/// Resolve a call (with the target code address).
	#[allow(clippy::type_complexity)]
	fn resolve_call(
		&self,
		code_address: H160,
		input: Vec<u8>,
		mut state: S,
		handler: &mut H,
	) -> Result<InvokerControl<Self::Interpreter, (ExitResult, (S, Vec<u8>))>, ExitError> {
		if let Some((r, retval)) =
			self.precompiles.execute(code_address, &input, &mut state, handler)
		{
			return Ok(InvokerControl::DirectExit((r, (state, retval))));
		}

		let code = handler.code(code_address);

		let machine = Machine::<S>::new(
			Rc::new(code),
			Rc::new(input),
			self.config.stack_limit,
			self.config.memory_limit,
			state,
		);

		let ret = InvokerControl::Enter(EtableInterpreter::new(machine, self.etable));

		Ok(ret)
	}

	/// Resolve a create (with the init code).
	#[allow(clippy::type_complexity)]
	fn resolve_create(
		&self,
		init_code: Vec<u8>,
		state: S,
		_handler: &mut H,
	) -> Result<InvokerControl<Self::Interpreter, (ExitResult, (S, Vec<u8>))>, ExitError> {
		let machine = Machine::new(
			Rc::new(init_code),
			Rc::new(Vec::new()),
			self.config.stack_limit,
			self.config.memory_limit,
			state,
		);

		let ret = InvokerControl::Enter(EtableInterpreter::new(machine, self.etable));

		Ok(ret)
	}
}
