mod error;
mod logic;
mod syscall_math;
mod syscall_memory;
mod util;

use crate::{
	logic::{
		emit_event_experimental, env_address_balance, env_block_hash, env_block_number,
		env_block_timestamp, env_call_contract, env_call_contract2, env_caller_address,
		env_contract_code_address_of, env_contract_code_owner_address_of,
		env_contract_instance_address, env_contract_owner_address, env_contract_owner_address_of,
		env_current_runtime_version, env_gas_left, env_gas_limit, env_input, env_msg, env_output,
		env_panic, env_panic_msg, env_read_register, env_register_len, env_storage_read,
		env_storage_remove, env_storage_write, env_storage_write_perm, env_transfer_from_caller,
		env_transfer_to, env_write_register, VMLogic,
	},
	syscall_math::syscall_math_op64,
	syscall_memory::{syscall_memory_op64, WASM_PAGE_SIZE},
};
use anyhow::anyhow;
use binread::BinRead;
use byteorder::{ByteOrder, LittleEndian};
use goblin::{
	elf::{Elf, SectionHeader},
	elf64::section_header::SHN_UNDEF,
};
use hash32::{Hasher, Murmur3Hasher};
use l1x_consensus_primitives::{
	Address, Balance, BlockHash, BlockNumber, BlockTimeStamp, ContractArgument, ContractCode, ContractFunction, Gas
};
use l1x_consensus_traits::traits_l1xvm::VMContractTrait;
use log::{debug, warn};
use logic::{env_deposit, Memory, CURRENT_RUNTIME_VERSION, RUNTIME_VERSION_DEPOSIT_FEATURE};
use rbpf::EbpfVmMbuff;
use vm_execution_fee::execution_fees::VmExecutionFees;

use std::{
	cell::{RefCell, UnsafeCell},
	hash::Hash,
	io::Cursor,
	os::raw::c_void,
	sync::Arc,
};

const R_BPF_64_NONE: u32 = 0;
const R_BPF_64_64: u32 = 1;
const R_BPF_64_ABS64: u32 = 2;
const R_BPF_64_RELATIVE: u32 = 8;
const R_BPF_64_32: u32 = 10;

const BYTE_OFFSET_IMMEDIATE: usize = 4;
const BYTE_LENGTH_IMMEDIATE: usize = 4;

const INSTRUCTION_SIZE: usize = 8;

// .data memory region is located in mbuff offset by 1024
const MBUFF_DATA_START: usize = 0;
// .rodata memory region is located in mbuff offset by 4096
const MBUFF_RODATA_START: usize = 0;

const CURRENT_OBJECT_VERSION: u64 = 1;
const MAX_SHARED_MEMORY_PAGES: u64 = 320; // 20 Mib

#[derive(Debug)]
pub struct VMOutcome {
	pub result: String,
	pub burnt_gas: Gas,
}

#[derive(Debug)]
pub struct VMOutcomeInternal {
	vm_outcome: VMOutcome,
	vm_return_code: u64,
}

#[derive(BinRead, Debug)]
struct EbpfObjectVersion {
	object_version: u64,
	expected_rutime_version: u64,
}

impl EbpfObjectVersion {
	pub fn packed_size() -> usize {
		// Size of Version if it was #[repr(packed)]
		16
	}
}

fn verifier(_prog: &[u8]) -> Result<(), std::io::Error> {
	Ok(())
}

fn section(elf: &Elf, name: &str) -> anyhow::Result<SectionHeader> {
	match elf.section_headers.iter().find(|section_header| {
		if let Some(this_name) = elf.shdr_strtab.get_at(section_header.sh_name) {
			return this_name == name
		}
		false
	}) {
		Some(section) => Ok(section.clone()),
		None => anyhow::bail!("section '{}' not found", name),
	}
}

fn hash_syscall_name(name: &str) -> u32 {
	let mut hasher = Murmur3Hasher::default();
	Hash::hash_slice(name.as_bytes(), &mut hasher);
	hasher.finish32()
}

fn is_allowed_syscall(object_version: &EbpfObjectVersion, name: &str) -> anyhow::Result<()> {
	if object_version.expected_rutime_version >= RUNTIME_VERSION_DEPOSIT_FEATURE {
		let disallowed_syscall = "env_transfer_from_caller"; 
		if name == disallowed_syscall {
			Err(anyhow!("{} is not allowed", disallowed_syscall))?;
		}
	}
	Ok(())
}

// Relocates all entries relative to `mbuff`
fn relocate(elf: &Elf, elf_bytes: &mut [u8], mbuff: &mut Vec<u8>) -> anyhow::Result<()> {
	let text_section = section(&elf, ".text")?;
	let data_section = section(&elf, ".data")?;
	let object_version = find_version(elf, elf_bytes)?;
	// Populate mbuff data region with bytes from .data section
	let data_bytes = elf_bytes.get(data_section.file_range().unwrap_or_default()).unwrap();

	mbuff.resize(MBUFF_DATA_START + data_bytes.len(), 0);
	mbuff[MBUFF_DATA_START..MBUFF_DATA_START + data_bytes.len()].clone_from_slice(data_bytes);

	for relocation in elf.shdr_relocs.iter().map(|(_, s)| s.iter()).flatten() {
		let r_offset = relocation.r_offset as usize;
		match relocation.r_type {
			R_BPF_64_NONE => anyhow::bail!("64_NONE is unsupported"),
			R_BPF_64_64 => {
				debug!("R_BPF_64_64");
				let imm_offset = text_section.sh_offset as usize + r_offset + BYTE_OFFSET_IMMEDIATE;
				debug!("offset: {}", imm_offset);

				let checked_slice = elf_bytes
					.get(imm_offset..imm_offset.saturating_add(BYTE_LENGTH_IMMEDIATE))
					.ok_or(anyhow::anyhow!("value out of bounds"))?;
				debug!("checked_slice: {checked_slice:?}");
				let refd_addr = LittleEndian::read_u32(checked_slice) as u64;
				debug!("refd_addr: {refd_addr}");

				let symbol = elf
					.syms
					.get(relocation.r_sym as usize)
					.ok_or_else(|| anyhow::anyhow!("unknown symbol: {}", relocation.r_sym))?;
				debug!("symbol: {symbol:?}");

				let mut addr = symbol.st_value.saturating_add(refd_addr);

				if addr < mbuff.as_ptr() as u64 {
					addr = (mbuff.as_ptr() as u64)
						.saturating_add(MBUFF_DATA_START as u64)
						.saturating_add(addr);
				}

				// LDDW loads instruction loads like this:
				//
				// 18 02 00 00 < 01 23 45 67 >
				// 00 00 00 00 < 89 AB CD EF >
				//
				// Where 01234567 is the lower half of the address to load and
				// 89ABCDEF is the higher half of the address to load. There are
				// 4 zero bytes in-between, this is intended.
				let imm_low_offset = imm_offset;
				let imm_high_offset = imm_low_offset.saturating_add(INSTRUCTION_SIZE);

				let imm_slice = elf_bytes
					.get_mut(imm_low_offset..imm_low_offset.saturating_add(BYTE_LENGTH_IMMEDIATE))
					.ok_or(anyhow::anyhow!("value out of bounds"))?;
				LittleEndian::write_u32(imm_slice, (addr & 0xFFFFFFFF) as u32);

				let imm_slice = elf_bytes
					.get_mut(imm_high_offset..imm_high_offset.saturating_add(BYTE_LENGTH_IMMEDIATE))
					.ok_or(anyhow::anyhow!("value out of bounds"))?;
				LittleEndian::write_u32(imm_slice, addr.checked_shr(32).unwrap_or_default() as u32);
			},
			R_BPF_64_ABS64 => {
				debug!("R_BPF_64_ABS64");
				let imm_offset = text_section.sh_offset as usize + r_offset;
				debug!("offset: {:#02x}", imm_offset);

				let checked_slice = elf_bytes
					.get(imm_offset..imm_offset.saturating_add(8))
					.ok_or(anyhow::anyhow!("value out of bounds"))?;
				debug!("checked_slice: {checked_slice:?}");
				let refd_addr = LittleEndian::read_u64(checked_slice) as u64;
				debug!("refd_addr: {refd_addr}");

				let symbol = elf
					.syms
					.get(relocation.r_sym as usize)
					.ok_or_else(|| anyhow::anyhow!("unknown symbol: {}", relocation.r_sym))?;
				debug!("symbol: {symbol:?}");

				let imm_offset =
					(elf.section_headers[symbol.st_shndx].sh_offset + symbol.st_value) as usize;
				let mut i = 0;
				debug!("offset: {:#02x}", imm_offset);
				loop {
					let byte = elf_bytes[imm_offset + i];
					if byte == 0 {
						break
					}

					mbuff[MBUFF_RODATA_START + (symbol.st_value as usize) + i] = byte;
					i += 1;
				}
				let mbuff_ptr = mbuff.as_ptr();
				let offset_low = MBUFF_DATA_START + r_offset;
				let offset_high = MBUFF_DATA_START + r_offset + 8;
				debug!("offset_low: {:#02x}", (mbuff_ptr as usize) + offset_low);
				let mbuff_slice = mbuff
					.get_mut(offset_low..offset_high)
					.ok_or(anyhow::anyhow!("value out of bounds"))?;
				LittleEndian::write_u64(
					mbuff_slice,
					(mbuff_ptr as usize + MBUFF_RODATA_START + (symbol.st_value as usize)) as u64,
				);
			},
			R_BPF_64_RELATIVE => anyhow::bail!("64_RELATIVE is unsupported"),
			R_BPF_64_32 => {
				let imm_offset = text_section.sh_offset as usize +
					r_offset.saturating_add(BYTE_OFFSET_IMMEDIATE);

				let symbol = elf
					.syms
					.get(relocation.r_sym)
					.ok_or_else(|| anyhow::anyhow!("unknown symbol: {}", relocation.r_sym))?;

				let name = elf
					.strtab
					.get_at(symbol.st_name)
					.ok_or_else(|| anyhow::anyhow!("unknown symbol: {}", relocation.r_sym))?;

				let key = if symbol.is_function() &&
					symbol.st_shndx != <u32 as TryInto<usize>>::try_into(SHN_UNDEF).unwrap()
				{
					if !text_section.vm_range().contains(&(symbol.st_value as usize)) {
						anyhow::bail!("value out of bounds");
					}
					// Calculate offset from target_pc. The result should be divinded by 8
					let target_pc: i64 = if symbol.st_value > (relocation.r_offset + 8) {
						(symbol.st_value - (relocation.r_offset + 8)) as i64
					} else {
						((relocation.r_offset + 8) - symbol.st_value) as i64 * -1
					};
					(target_pc / 8) as u32
				} else {
					let src_offset =
						r_offset.saturating_add(text_section.sh_offset as usize).saturating_add(1);
					// Mark as a system call by setting src to 0
					elf_bytes[src_offset] = 0;

					is_allowed_syscall(&object_version, name)?;
					hash_syscall_name(name)
				};

				let checked_slice = elf_bytes
					.get_mut(imm_offset..imm_offset.saturating_add(BYTE_LENGTH_IMMEDIATE))
					.ok_or(anyhow::anyhow!("value out of bounds"))?;
				LittleEndian::write_u32(checked_slice, key);
			},
			_ => anyhow::bail!("unknown relocation type: {}", relocation.r_type),
		};
	}

	Ok(())
}

fn find_function(elf: &Elf, name: &ContractFunction) -> anyhow::Result<u64> {
	let offset = elf.syms.iter().find_map(|sym| {
		if sym.is_function() {
			let sym_name = elf.strtab.get_at(sym.st_name);
			if let Some(sym_name) = sym_name {
				if sym_name.as_bytes() == name {
					return Some(sym.st_value)
				}
			}
		}
		None
	});

	if let Some(offset) = offset {
		Ok(offset)
	} else {
		let name = String::from_utf8_lossy(name);
		Err(anyhow::anyhow!("Can't find function: {name}"))
	}
}

fn create_shared_memory(elf: &Elf, elf_bytes: &[u8]) -> anyhow::Result<Memory> {
	let memory_section_name = "_memory";
	let init_memory_section_name = "_init_memory";

	#[derive(BinRead, Debug)]
	struct MemoryLimit {
		min_pages: u64,
		max_pages: u64,
	}
	impl MemoryLimit {
		pub fn packed_size() -> usize {
			// Size of MemoryLimit if it was #[repr(packed)]
			16
		}
	}
	#[derive(BinRead, Debug)]
	struct InitMemory {
		memory_index: u32,
		offset: u64,
		_data_len: u64,
		#[br(count = _data_len)]
		data: Vec<u8>,
	}

	let memory_section = match section(elf, memory_section_name) {
		Ok(v) => v,
		Err(_) => {
			warn!("Can't find {memory_section_name} section");
			return Ok(Memory { max_pages: 0, shared_memory_buffer: Vec::new() })
		},
	};

	if memory_section.sh_size as usize % MemoryLimit::packed_size() != 0 ||
		memory_section.sh_size == 0
	{
		anyhow::bail!("Incorrect size of {memory_section_name} section");
	}

	if memory_section.sh_size / MemoryLimit::packed_size() as u64 != 1 {
		anyhow::bail!("Many memory records are not supported");
	}

	debug!(
		"{} section sh_size={} sh_offset={}",
		memory_section_name, memory_section.sh_size, memory_section.sh_offset,
	);

	let memory_limit = {
		let start_offset = memory_section.sh_offset as usize;
		let end_offset = memory_section
			.sh_offset
			.checked_add(memory_section.sh_size)
			.ok_or_else(|| anyhow::anyhow!("Memory section is too big"))? as usize;
		let slice = elf_bytes
			.get(start_offset..end_offset)
			.ok_or_else(|| anyhow::anyhow!("value out of bounds"))?;

		let mut reader = Cursor::new(slice);
		MemoryLimit::read(&mut reader)?
	};

	// memory_limit.max_pages is checked in syscall_memory.rs when MEMORY_GROW is handled
	if memory_limit.min_pages > MAX_SHARED_MEMORY_PAGES ||
		memory_limit.max_pages > MAX_SHARED_MEMORY_PAGES
	{
		let required_memory = memory_limit.min_pages.max(memory_limit.max_pages);
		return Err(anyhow::anyhow!("Shared memory limit is {MAX_SHARED_MEMORY_PAGES} pages, the contract requires {required_memory} pages"));
	}

	debug!("memory_limit: {:?}", memory_limit);

	let mut memory = {
		let memory_limit_max = if memory_limit.max_pages == 0 {
			MAX_SHARED_MEMORY_PAGES
		} else {
			memory_limit.max_pages
		};

		Memory {
			max_pages: memory_limit_max,
			shared_memory_buffer: vec![0u8; memory_limit.min_pages as usize * WASM_PAGE_SIZE],
		}
	};

	if let Ok(init_memory_section) = section(elf, init_memory_section_name) {
		debug!(
			"{} section: sh_size={} sh_offset={}",
			init_memory_section_name, init_memory_section.sh_size, init_memory_section.sh_offset
		);
		let start_offset = init_memory_section.sh_offset as usize;
		let end_offset = init_memory_section
			.sh_offset
			.checked_add(init_memory_section.sh_size)
			.ok_or_else(|| anyhow::anyhow!("Memory section is too big"))? as usize;
		let mem_init_slice = elf_bytes
			.get(start_offset..end_offset)
			.ok_or_else(|| anyhow::anyhow!("value out of bounds"))?;

		let mut reader = Cursor::new(mem_init_slice);
		while (reader.position() as usize) < mem_init_slice.len() {
			let init_mem = InitMemory::read(&mut reader)?;

			if init_mem.memory_index != 0 {
				anyhow::bail!("Incorrect memory index")
			}

			debug!("init_memory: {:?}", init_mem);

			memory
				.shared_memory_buffer
				.iter_mut()
				.skip(init_mem.offset as _)
				.zip(init_mem.data.iter())
				.for_each(|(dst, src)| {
					*dst = *src;
				});
		}
	} else {
		warn!("{init_memory_section_name} section has not been found");
	}

	Ok(memory)
}

fn find_version(elf: &Elf, elf_bytes: &[u8]) -> anyhow::Result<EbpfObjectVersion> {
	let version_section_name = "_version";

	let version_section = section(elf, version_section_name)?;
	if version_section.sh_size != EbpfObjectVersion::packed_size() as u64 {
		anyhow::bail!("Incorrect size of {version_section_name} section");
	}

	let version = {
		let start_offset = version_section.sh_offset as usize;
		let end_offset = version_section
			.sh_offset
			.checked_add(version_section.sh_size)
			.ok_or_else(|| anyhow::anyhow!("Version section is too big"))? as usize;
		let slice = elf_bytes
			.get(start_offset..end_offset)
			.ok_or_else(|| anyhow::anyhow!("value out of bounds"))?;

		let mut reader = Cursor::new(slice);
		EbpfObjectVersion::read(&mut reader)?
	};

	Ok(version)
}

fn verify_version(version: &EbpfObjectVersion) -> anyhow::Result<()> {
	if version.object_version != CURRENT_OBJECT_VERSION {
		return Err(anyhow::anyhow!(
			"Unsupported version of the object '{}', the expected version is '{}'",
			version.object_version,
			CURRENT_OBJECT_VERSION
		))
	}
	if version.expected_rutime_version == 0 ||
		version.expected_rutime_version > CURRENT_RUNTIME_VERSION
	{
		return Err(anyhow::anyhow!(
			"Unsupported version of the runtime '{}', the current runtime version is '{}'",
			version.expected_rutime_version,
			CURRENT_RUNTIME_VERSION
		))
	}

	Ok(())
}

fn verify_shared_memory(elf: &Elf, elf_bytes: &[u8]) -> anyhow::Result<()> {
	create_shared_memory(elf, elf_bytes)?;
	Ok(())
}

pub fn verify_ebpf(elf_bytes: &Vec<u8>) -> anyhow::Result<()> {
	let elf_bytes_clone = elf_bytes.clone();
	let elf = Elf::parse(&elf_bytes)?;
	let mut elf_bytes = elf_bytes_clone;

	let mut mbuff = UnsafeCell::new(Vec::new());
	relocate(&elf, &mut elf_bytes, mbuff.get_mut())?;
	let relocated_elf_bytes = elf_bytes;

	// std::fs::write("tmp.o", relocated_elf_bytes.clone())?;
	let elf = Elf::parse(&relocated_elf_bytes)?;
	let text_section = section(&elf, ".text")?;

	let text_bytes =
		relocated_elf_bytes.get(text_section.file_range().unwrap_or_default()).unwrap();

	let _ = EbpfVmMbuff::new(Some(&text_bytes))?;

	let object_version = find_version(&elf, &relocated_elf_bytes)?;
	verify_version(&object_version)?;
	verify_shared_memory(&elf, &relocated_elf_bytes)?;

	Ok(())
}

pub fn run_with_registers<'a>(
	elf_bytes: &ContractCode,
	fn_name: ContractFunction,
	input: ContractArgument,
	caller_address: Address,
	contract_code_address: Address,
	contract_instance_address: Address,
	contract_owner_address: Address,
	block_number: BlockNumber,
	block_hash: BlockHash,
	block_timestamp: BlockTimeStamp,
	readonly: bool,
	gas_limit: Gas,
	despoit: Balance,
	fee_config: VmExecutionFees,
	vm_api: Arc<RefCell<&'a mut dyn VMContractTrait>>,
	register_args: Option<Vec<u64>>,
) -> anyhow::Result<VMOutcomeInternal> {
	run_internal(
		elf_bytes,
		fn_name,
		input,
		caller_address,
		contract_code_address,
		contract_instance_address,
		contract_owner_address,
		block_number,
		block_hash,
		block_timestamp,
		readonly,
		gas_limit,
		despoit,
		fee_config,
		vm_api,
		register_args,
	)
}

pub fn run<'a>(
	elf_bytes: &ContractCode,
	fn_name: ContractFunction,
	input: ContractArgument,
	caller_address: Address,
	contract_code_address: Address,
	contract_instance_address: Address,
	contract_owner_address: Address,
	block_number: BlockNumber,
	block_hash: BlockHash,
	block_timestamp: BlockTimeStamp,
	readonly: bool,
	gas_limit: Gas,
	deposit: Balance,
	fee_config: VmExecutionFees,
	vm_api: Arc<RefCell<&'a mut dyn VMContractTrait>>,
) -> anyhow::Result<VMOutcome> {
	Ok(run_internal(
		elf_bytes,
		fn_name,
		input,
		caller_address,
		contract_code_address,
		contract_instance_address,
		contract_owner_address,
		block_number,
		block_hash,
		block_timestamp,
		readonly,
		gas_limit,
		deposit,
		fee_config,
		vm_api,
		None,
	)?
	.vm_outcome)
}

fn run_internal<'a>(
	elf_bytes: &ContractCode,
	fn_name: ContractFunction,
	input: ContractArgument,
	caller_address: Address,
	contract_code_address: Address,
	contract_instance_address: Address,
	contract_owner_address: Address,
	block_number: BlockNumber,
	block_hash: BlockHash,
	block_timestamp: BlockTimeStamp,
	readonly: bool,
	gas_limit: Gas,
	deposit: Balance,
	fee_config: VmExecutionFees,
	vm_api: Arc<RefCell<&'a mut dyn VMContractTrait>>,
	register_args: Option<Vec<u64>>,
) -> anyhow::Result<VMOutcomeInternal> {
	// FIXME: commenting out as seems to affect the entire node and not just contract execution
	// match SimpleLogger::new().with_level(LevelFilter::Warn).init() {
	//     Ok(_) => (),
	//     Err(e) => eprintln!("WARN: Can't init logger: {}", e),
	// }

	let elf_bytes_clone = elf_bytes.clone();
	let elf = Elf::parse(&elf_bytes)?;
	let mut elf_bytes = elf_bytes_clone;
	let object_version = find_version(&elf, &elf_bytes)?;

	verify_version(&object_version)?;

	let text_section = section(&elf, ".text")?;
	debug!("text section: {:?}", text_section.file_range().unwrap_or_default());
	let data_section = section(&elf, ".data")?;
	debug!("data section: {:?}", data_section.file_range().unwrap_or_default());

	let mut mbuff = UnsafeCell::new(Vec::new());
	relocate(&elf, &mut elf_bytes, mbuff.get_mut())?;
	let relocated_elf_bytes = elf_bytes;

	// std::fs::write("tmp.o", relocated_elf_bytes.clone())?;
	let elf = Elf::parse(&relocated_elf_bytes)?;
	let text_section = section(&elf, ".text")?;

	let text_bytes =
		relocated_elf_bytes.get(text_section.file_range().unwrap_or_default()).unwrap();
	let start_offset = find_function(&elf, &fn_name)?;

	let mut vm = EbpfVmMbuff::new(Some(&text_bytes))?;
	vm.set_verifier(verifier)?;
	vm.register_helper_with_ctx(hash_syscall_name("syscall_math_op64"), syscall_math_op64)?;
	vm.register_helper_with_ctx(hash_syscall_name("syscall_math_op32"), syscall_math_op64)?;
	vm.register_helper_with_ctx(hash_syscall_name("syscall_memory_op64"), syscall_memory_op64)?;
	vm.register_helper_with_ctx(hash_syscall_name("syscall_memory_op32"), syscall_memory_op64)?;

	vm.register_helper_with_ctx(hash_syscall_name("env_msg"), env_msg)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_panic"), env_panic)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_panic_msg"), env_panic_msg)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_input"), env_input)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_register_len"), env_register_len)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_read_register"), env_read_register)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_write_register"), env_write_register)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_output"), env_output)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_storage_write"), env_storage_write)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_storage_remove"), env_storage_remove)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_storage_read"), env_storage_read)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_storage_write_perm"),
		env_storage_write_perm,
	)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_current_runtime_version"),
		env_current_runtime_version,
	)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_caller_address"), env_caller_address)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_contract_owner_address"),
		env_contract_owner_address,
	)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_contract_owner_address_of"),
		env_contract_owner_address_of,
	)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_contract_instance_address"),
		env_contract_instance_address,
	)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_contract_code_address_of"),
		env_contract_code_address_of,
	)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_contract_code_owner_address_of"),
		env_contract_code_owner_address_of,
	)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_gas_limit"), env_gas_limit)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_gas_left"), env_gas_left)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_deposit"), env_deposit)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_address_balance"), env_address_balance)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_transfer_to"), env_transfer_to)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_transfer_from_caller"),
		env_transfer_from_caller,
	)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_block_number"), env_block_number)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_block_timestamp"), env_block_timestamp)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_block_hash"), env_block_hash)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_call_contract"), env_call_contract)?;
	vm.register_helper_with_ctx(hash_syscall_name("env_call_contract2"), env_call_contract2)?;
	vm.register_helper_with_ctx(
		hash_syscall_name("env_emit_event_experimental"),
		emit_event_experimental,
	)?;

	let shared_memory_buffer = create_shared_memory(&elf, &relocated_elf_bytes)?;
	let mut vm_logic = VMLogic::new(
		input.clone(),
		caller_address,
		contract_code_address,
		contract_instance_address,
		contract_owner_address,
		shared_memory_buffer,
		block_number,
		block_hash,
		block_timestamp,
		readonly,
		gas_limit,
		deposit,
		fee_config.clone(),
		object_version,
		vm_api,
	);
	{
		let raw_ptr = &mut vm_logic as *mut _ as *mut c_void;
		let _ = vm.set_ctx(raw_ptr as usize);
	}

	let mem = &mut [0x00; 1024];
	if let Some(args) = register_args {
		let bytes = args.iter().map(|x| x.to_ne_bytes()).flat_map(|v| v).collect::<Vec<_>>();
		mem.iter_mut().zip(bytes.iter()).for_each(|(dst, src)| *dst = *src);
	}
	let vm_return_code = vm.execute_program(
		mem,
		mbuff.get_mut(),
		start_offset,
		fee_config.vm_fee,
		vm_logic.runtime_gas_ref(),
	)?;
	let ret = {
		let burnt_gas: Gas = vm_logic.runtime_gas_ref().borrow().burnt_gas;
		let return_data = &vm_logic.return_data;
		if return_data.len() > 0 {
			Ok(VMOutcomeInternal {
				vm_outcome: VMOutcome {
					result: std::str::from_utf8(return_data)?.to_string(),
					burnt_gas,
				},
				vm_return_code,
			})
		} else {
			Ok(VMOutcomeInternal {
				vm_outcome: VMOutcome { result: "".to_string(), burnt_gas },
				vm_return_code,
			})
		}
	};

	ret
}
