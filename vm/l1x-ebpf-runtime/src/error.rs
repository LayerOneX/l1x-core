#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum HostError {
	#[error("VM2Host IO Error: InValid Ctx {0}")]
	InValidCtx(usize),
	#[error("VM2Host IO Error: Guest Panic {panic_msg:?}")]
	GuestPanic { panic_msg: String },
	#[error("VM2Host IO Error: Memory Access Violation")]
	MemoryAccessViolation,
	#[error("VM2Host IO Error: Invalid Register ID {register_id:?}")]
	InvalidRegisterId { register_id: u64 },
	#[error("VM2Host IO Error: MalFormed UTF8")]
	MalformedUtf8,
	#[error("VM2Host IO Error: Storage Error {msg:?}")]
	StorageError { msg: String },
	#[error(
        "VM2Host IO Error: The called method tried to change the state or did a mutable cross-contract call without write permissions"
    )]
	ReadonlyCall,
	#[error("VM2Host IO Error: The address length is invalid")]
	InvalidAddress,
	#[error("VM2Host IO Error: The syscall's argument is invalid")]
	InvalidArgument,
	#[error("VM2Host IO Error: Invalid syscall memory operation: op={op}, mem_addr={mem_addr:#x} mem_offset={mem_offset:#x}")]
	MemorySyscallError { op: u64, mem_addr: usize, mem_offset: usize },
	#[error("VM2Host IO Error: Out of memory limit: limit={max_pages} pages, allocated={allocated_pages} pages, requested={requested_pages} pages")]
	OutOfMemoryLimit { max_pages: u64, allocated_pages: u64, requested_pages: u64 },
	#[error("Out of Gas")]
	OutOfGas,
	#[error("Contract call Gas limit is too big: gas_left={gas_left}, gas_limit={gas_limit}")]
	ContractCallGasLimit{gas_left: u64, gas_limit: u64},
	#[error("Contract call deposit is too big: deposit={deposit}")]
	ContractCallDeposit{deposit: u128},
	#[error("API method is not supported: {0}")]
	NotSupported(String)
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum VMLogicError {
	#[error("VMLogicError IO: {0}")]
	HostError(HostError),
}

impl From<HostError> for VMLogicError {
	fn from(err: HostError) -> Self {
		VMLogicError::HostError(err)
	}
}
