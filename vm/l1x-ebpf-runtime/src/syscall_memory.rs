use log::debug;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use rbpf::ebpf::HelperResult;

pub const WASM_PAGE_SIZE: usize = 65536;

use crate::{error, logic::VMLogic};

#[derive(FromPrimitive, Debug)]
enum SyscallMemoryOp {
	I32Load = 1,
	I32Load16S,
	I32Load16U,
	I32Load8S,
	I32Load8U,
	I64Load,
	I64Load32S,
	I64Load32U,
	I64Load16S,
	I64Load16U,
	I64Load8S,
	I64Load8U,
	F64Load,
	F32Load,
	I32Store,
	I32Store16,
	I32Store8,
	I64Store,
	I64Store32,
	I64Store16,
	I64Store8,
	F32Store,
	F64Store,
	MemoryGrow,
	MemorySize,
}

pub fn syscall_memory_op64(
	ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	r4: u64,
	_r5: u64,
) -> HelperResult {
	let vm_logic =
		VMLogic::from_ptr(ctx).ok_or_else(|| error::HostError::InValidCtx(ctx).to_string())?;
	let memory = &mut vm_logic.memory;

	let op = r1;
	let arg1 = r2;
	let arg2 = r3;
	let arg3 = r4;

	let mem_addr = arg1 as usize;
	let offset = arg2 as usize;
	let number = arg3;
	let mem_offset = mem_addr + offset;

	let stringify_error =
		|_| error::HostError::MemorySyscallError { op, mem_addr, mem_offset }.to_string();

	let read_byte = || memory.read_byte(mem_offset).map_err(stringify_error);
	let read_bytes2 = || memory.read_bytes2(mem_offset).map_err(stringify_error);
	let read_bytes4 = || memory.read_bytes4(mem_offset).map_err(stringify_error);
	let read_bytes8 = || memory.read_bytes8(mem_offset).map_err(stringify_error);

	//println!("MEMORY OP {op} {mem_addr} {offset} {number}");

	let res = match FromPrimitive::from_u64(op) {
		Some(SyscallMemoryOp::MemoryGrow) => {
			debug!("MEMORY GROW {arg1}");
			let old_size = vm_logic.memory.shared_memory_buffer.len();
			// old_size + (arg1 as usize * WASM_PAGE_SIZE)
			let new_size = (arg1 as usize)
				.checked_mul(WASM_PAGE_SIZE)
				.and_then(|diff_size| old_size.checked_add(diff_size));

			let out_of_limit_error = Err(error::HostError::OutOfMemoryLimit {
				max_pages: vm_logic.memory.max_pages,
				allocated_pages: (old_size / WASM_PAGE_SIZE) as u64,
				requested_pages: arg1,
			}
			.to_string());

			if let Some(new_size) = new_size {
				if new_size <= vm_logic.memory.max_pages as usize * WASM_PAGE_SIZE {
					vm_logic.memory.shared_memory_buffer.resize(new_size, 0);

					(old_size / WASM_PAGE_SIZE) as u64
				} else {
					// By WASM standard, -1i32 should be returned.
					// But in real wasm files, the "-1" value is not handled correctly and it leads
					// to out-of-bound errors. To help developers to debug their contracts, the
					// contract will be stopped with this error.
					//
					// (-1i32) as u64
					return out_of_limit_error;
				}
			} else {
				// (-1i32) as u64
				return out_of_limit_error;
			}
		},
		Some(SyscallMemoryOp::MemorySize) => {
			let size = vm_logic.memory.shared_memory_buffer.len();

			(size / WASM_PAGE_SIZE) as u64
		},
		// Load
		Some(SyscallMemoryOp::I32Load) => u32::from_ne_bytes(read_bytes4()?) as u64,
		Some(SyscallMemoryOp::I32Load16S) => (i16::from_ne_bytes(read_bytes2()?) as i32) as u64,
		Some(SyscallMemoryOp::I32Load16U) => (u16::from_ne_bytes(read_bytes2()?) as u32) as u64,
		Some(SyscallMemoryOp::I32Load8S) => (i8::from_ne_bytes(read_byte()?) as i32) as u64,
		Some(SyscallMemoryOp::I32Load8U) => (u8::from_ne_bytes(read_byte()?) as u32) as u64,
		Some(SyscallMemoryOp::I64Load) => u64::from_ne_bytes(read_bytes8()?),
		Some(SyscallMemoryOp::I64Load32S) => (i32::from_ne_bytes(read_bytes4()?) as i64) as u64,
		Some(SyscallMemoryOp::I64Load32U) => (u32::from_ne_bytes(read_bytes4()?) as u64) as u64,
		Some(SyscallMemoryOp::I64Load16S) => (i16::from_ne_bytes(read_bytes2()?) as i64) as u64,
		Some(SyscallMemoryOp::I64Load16U) => (u16::from_ne_bytes(read_bytes2()?) as u64) as u64,
		Some(SyscallMemoryOp::I64Load8S) => (i8::from_ne_bytes(read_byte()?) as i64) as u64,
		Some(SyscallMemoryOp::I64Load8U) => (u8::from_ne_bytes(read_byte()?) as u64) as u64,
		Some(SyscallMemoryOp::F64Load) => u64::from_ne_bytes(read_bytes8()?),
		Some(SyscallMemoryOp::F32Load) => u32::from_ne_bytes(read_bytes4()?) as u64,
		// Store
		Some(SyscallMemoryOp::F64Store) | Some(SyscallMemoryOp::I64Store) => {
			let n: u64 = number;
			memory.write_bytes8(mem_offset, n.to_ne_bytes()).map_err(stringify_error)?;
			0
		},
		Some(SyscallMemoryOp::F32Store)
		| Some(SyscallMemoryOp::I64Store32)
		| Some(SyscallMemoryOp::I32Store) => {
			let n = number as u32;
			memory.write_bytes4(mem_offset, n.to_ne_bytes()).map_err(stringify_error)?;
			0
		},
		Some(SyscallMemoryOp::I64Store16) | Some(SyscallMemoryOp::I32Store16) => {
			let n = number as u16;
			memory.write_bytes2(mem_offset, n.to_ne_bytes()).map_err(stringify_error)?;
			0
		},
		Some(SyscallMemoryOp::I64Store8) | Some(SyscallMemoryOp::I32Store8) => {
			let n = number as u8;
			memory.write_byte(mem_offset, n.to_ne_bytes()).map_err(stringify_error)?;
			0
		},
		_ => Err(format!("Unknown syscall memory op={op}"))?,
	};

	Ok(res)
}
