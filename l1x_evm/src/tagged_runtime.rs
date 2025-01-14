//! A module containing data types for keeping track of the kinds of calls
//! (CALL vs CREATE) in the EVM call stack.

use evm::maybe_borrowed::MaybeBorrowed;
use evm_runtime::{CreateScheme, Runtime};
use primitive_types::{H160, H256};
use primitives::Nonce;
use sha3::{Digest, Keccak256};

pub struct TaggedRuntime<'borrow> {
	pub kind: RuntimeKind,
	pub inner: MaybeBorrowed<'borrow, Runtime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
	Create(H160),
	Call(H160),
	// Special variant used only in `StackExecutor::execute`
	//Execute,
}

// Get the create address from given scheme.
pub fn create_evm_address(scheme: CreateScheme, nonce: Nonce) -> H160 {
	match scheme {
		CreateScheme::Create2 { caller, code_hash, salt } => {
			let mut hasher = Keccak256::new();
			hasher.update([0xff]);
			hasher.update(&caller[..]);
			hasher.update(&salt[..]);
			hasher.update(&code_hash[..]);
			H256::from_slice(hasher.finalize().as_slice()).into()
		},
		CreateScheme::Legacy { caller } => {
			let mut stream = rlp::RlpStream::new_list(2);
			stream.append(&caller);
			stream.append(&nonce);
			H256::from_slice(Keccak256::digest(&stream.out()).as_slice()).into()
		},
		CreateScheme::Fixed(naddress) => naddress,
	}
}

pub struct RuntimeExecutorInterrupt<'borrow>(TaggedRuntime<'borrow>);
pub type CreateInterrupt = RuntimeExecutorInterrupt<'static>;
pub type CreateFeedback = RuntimeExecutorInterrupt<'static>;
pub type CallInterrupt = RuntimeExecutorInterrupt<'static>;
pub type CallFeedback = RuntimeExecutorInterrupt<'static>;
pub type RuntimeInterrupt = RuntimeExecutorInterrupt<'static>;
