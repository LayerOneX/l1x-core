use std::ops::Neg;

use log::debug;
use num_derive::FromPrimitive;
use num_traits::{Float, FromPrimitive, PrimInt, ToPrimitive};
use rbpf::ebpf::HelperResult;

#[derive(FromPrimitive, Debug)]
enum SyscallMathOp {
	F64ConvertI64U = 1,
	F64Add,
	F64Sub,
	F64Mul,
	F64Div,
	F64Eq,
	F64Ne,
	F64Neg,
	F64Gt,
	F64Ge,
	F64Le,
	F64Lt,
	F32Add,
	F32Sub,
	F32Mul,
	F32Div,
	F32Eq,
	F32Ne,
	F32Neg,
	F32Gt,
	F32Ge,
	F32Le,
	F32Lt,
	I64DivS,
	I32DivS,
	I64RemS,
	I32RemS,
	I64Clz,
	I32Clz,
	I64Ctz,
	I32Ctz,
	F64ConvertI32S,
	F64ConvertI32U,
	F64Abs,
	F32Abs,
	I32TruncF32S,
	I32TruncF32U,
	I32TruncF64S,
	I32TruncF64U,
	I64TruncF32S,
	I64TruncF32U,
	I64TruncF64S,
	I64TruncF64U,
	I32TruncSatF32S,
	I32TruncSatF32U,
	I32TruncSatF64S,
	I32TruncSatF64U,
	I64TruncSatF32S,
	I64TruncSatF32U,
	I64TruncSatF64S,
	I64TruncSatF64U,
}

impl Into<u64> for SyscallMathOp {
	fn into(self) -> u64 {
		self as u64
	}
}

pub fn syscall_math_op64(
	_ctx: usize,
	r1: u64,
	r2: u64,
	r3: u64,
	_r4: u64,
	_r5: u64,
) -> HelperResult {
	let op = r1;
	let arg1_u64 = r2;
	let arg2_u64 = r3;
	let arg1_f64 = f64::from_bits(r2);
	let arg2_f64 = f64::from_bits(r3);
	let arg1_f32 = f32::from_bits(r2 as u32);
	let arg2_f32 = f32::from_bits(r3 as u32);

	debug!("MATH: {:?}", op);
	let res = match FromPrimitive::from_u64(op) {
		Some(SyscallMathOp::F64ConvertI64U) => {
			f64::from_u64(arg1_u64).ok_or("Can't convert u64 to f64".to_string())?.to_bits()
		},
		Some(SyscallMathOp::F64Add) => {
			let res = arg1_f64 + arg2_f64;
			res.to_bits()
		},
		Some(SyscallMathOp::F64Sub) => {
			let res = arg1_f64 - arg2_f64;
			res.to_bits()
		},
		Some(SyscallMathOp::F64Mul) => {
			let res = arg1_f64 * arg2_f64;
			res.to_bits()
		},
		Some(SyscallMathOp::F64Div) => {
			let res = arg1_f64 / arg2_f64;
			res.to_bits()
		},
		Some(SyscallMathOp::F64Eq) => (arg1_f64 == arg2_f64) as u64,
		Some(SyscallMathOp::F64Ne) => (arg1_f64 != arg2_f64) as u64,
		Some(SyscallMathOp::F64Neg) => arg1_f64.neg().to_bits(),
		Some(SyscallMathOp::F64Abs) => arg1_f64.abs().to_bits(),
		Some(SyscallMathOp::F64Gt) => (arg1_f64 > arg2_f64) as u64,
		Some(SyscallMathOp::F64Ge) => (arg1_f64 >= arg2_f64) as u64,
		Some(SyscallMathOp::F64Le) => (arg1_f64 <= arg2_f64) as u64,
		Some(SyscallMathOp::F64Lt) => (arg1_f64 < arg2_f64) as u64,
		Some(SyscallMathOp::F32Add) => {
			let res = arg1_f32 + arg2_f32;
			res.to_bits() as u64
		},
		Some(SyscallMathOp::F32Sub) => {
			let res = arg1_f32 - arg2_f32;
			res.to_bits() as u64
		},
		Some(SyscallMathOp::F32Mul) => {
			let res = arg1_f32 * arg2_f32;
			res.to_bits() as u64
		},
		Some(SyscallMathOp::F32Div) => {
			let res = arg1_f32 / arg2_f32;
			res.to_bits() as u64
		},
		Some(SyscallMathOp::F32Eq) => (arg1_f32 == arg2_f32) as u64,
		Some(SyscallMathOp::F32Ne) => (arg1_f32 != arg2_f32) as u64,
		Some(SyscallMathOp::F32Neg) => arg1_f32.neg().to_bits() as u64,
		Some(SyscallMathOp::F32Abs) => arg1_f32.abs().to_bits() as u64,
		Some(SyscallMathOp::F32Gt) => (arg1_f32 > arg2_f32) as u64,
		Some(SyscallMathOp::F32Ge) => (arg1_f32 >= arg2_f32) as u64,
		Some(SyscallMathOp::F32Le) => (arg1_f32 <= arg2_f32) as u64,
		Some(SyscallMathOp::F32Lt) => (arg1_f32 < arg2_f32) as u64,
		Some(SyscallMathOp::I64DivS) => {
			let arg1_s = arg1_u64 as i64;
			let arg2_s = arg2_u64 as i64;

			if arg2_s == 0 {
				return Err(format!("Division by zero op={op}"));
			}

			(arg1_s / arg2_s) as u64
		},
		Some(SyscallMathOp::I32DivS) => {
			let arg1_s = arg1_u64 as i32;
			let arg2_s = arg2_u64 as i32;

			if arg2_s == 0 {
				return Err(format!("Division by zero op={op}"));
			}

			(arg1_s / arg2_s) as u64
		},
		Some(SyscallMathOp::I64RemS) => {
			let arg1_s = arg1_u64 as i64;
			let arg2_s = arg2_u64 as i64;

			if arg2_s == 0 {
				return Err(format!("Division by zero op={op}"));
			}

			(arg1_s % arg2_s) as u64
		},
		Some(SyscallMathOp::I32RemS) => {
			let arg1_s = arg1_u64 as i32;
			let arg2_s = arg2_u64 as i32;

			if arg2_s == 0 {
				return Err(format!("Division by zero op={op}"));
			}

			(arg1_s % arg2_s) as u64
		},
		Some(SyscallMathOp::I64Clz) => arg1_u64.leading_zeros() as u64,
		Some(SyscallMathOp::I32Clz) => (arg1_u64 as u32).leading_zeros() as u64,
		Some(SyscallMathOp::I64Ctz) => arg1_u64.trailing_zeros() as u64,
		Some(SyscallMathOp::I32Ctz) => (arg1_u64 as u32).trailing_zeros() as u64,
		Some(SyscallMathOp::F64ConvertI32S) => f64::from_i32(arg1_u64 as i32)
			.ok_or("Can't convert i32 to f64".to_string())?
			.to_bits(),
		Some(SyscallMathOp::F64ConvertI32U) => f64::from_u32(arg1_u64 as u32)
			.ok_or("Can't convert u32 to f64".to_string())?
			.to_bits(),
		Some(SyscallMathOp::I32TruncF64U) => float_trunc::<u32, f64>(arg1_f64)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I32TruncF64S) => float_trunc::<i32, f64>(arg1_f64)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I32TruncF32S) => float_trunc::<i32, f32>(arg1_f32)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I32TruncF32U) => float_trunc::<u32, f32>(arg1_f32)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I64TruncF32S) => float_trunc::<i64, f32>(arg1_f32)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I64TruncF32U) => float_trunc::<u64, f32>(arg1_f32)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I64TruncF64S) => float_trunc::<i64, f64>(arg1_f64)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I64TruncF64U) => float_trunc::<u64, f64>(arg1_f64)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I32TruncSatF64U) => float_trunc_sat::<u32, f64>(arg1_f64)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I32TruncSatF64S) => float_trunc_sat::<i32, f64>(arg1_f64)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I32TruncSatF32S) => float_trunc_sat::<i32, f32>(arg1_f32)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I32TruncSatF32U) => float_trunc_sat::<u32, f32>(arg1_f32)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I64TruncSatF32S) => float_trunc_sat::<i64, f32>(arg1_f32)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I64TruncSatF32U) => float_trunc_sat::<u64, f32>(arg1_f32)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I64TruncSatF64S) => float_trunc_sat::<i64, f64>(arg1_f64)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		Some(SyscallMathOp::I64TruncSatF64U) => float_trunc_sat::<u64, f64>(arg1_f64)
			.map_err(|e| format!("Operation {op:?} failed: {e}"))?,
		None => return Err(format!("Unknown syscall math operation: op={op}")),
	};

	Ok(res)
}

fn float_trunc<R, T>(arg: T) -> HelperResult
where
	T: Float,
	R: PrimInt,
{
	let truncated = arg.trunc();
	// infinity, neg_infinity, nan cases
	if !truncated.is_finite() {
		return Err(format!("Float point number is not finite"));
	}
	let integer = R::from(truncated).ok_or("Can't convert float to integer".to_string())?;
	if integer > R::zero() {
		Ok(integer.to_u64().ok_or("Can't convert truncated to integer to u64")?)
	} else {
		Ok(integer.to_i64().ok_or("Can't convert truncated to integer to i64")? as u64)
	}
}

fn float_trunc_sat<R, T>(arg: T) -> HelperResult
where
	T: Float + ToPrimitive,
	R: PrimInt + ToPrimitive,
{
	let truncated = arg.trunc();
	let integer = if truncated.is_nan() {
		R::zero()
	} else {
		match R::from(truncated) {
			Some(int) => int,
			None => {
				// infinity, neg_infinity, out of range cases
				if truncated.is_sign_positive() {
					R::max_value()
				} else {
					R::min_value()
				}
			},
		}
	};
	if integer > R::zero() {
		Ok(integer.to_u64().ok_or("Can't convert truncated to integer to u64")?)
	} else {
		Ok(integer.to_i64().ok_or("Can't convert truncated to integer to i64")? as u64)
	}
}

#[cfg(test)]
mod tests {
	use num_traits::ToPrimitive;
	use crate::syscall_math::{syscall_math_op64, SyscallMathOp};
	use std::panic;

	fn catch_panic<F: FnOnce() -> R + panic::UnwindSafe, R>(f: F) -> std::thread::Result<R> {
		let prev_hook = panic::take_hook();
		panic::set_hook(Box::new(|_| {}));
		let result = panic::catch_unwind(f);
		panic::set_hook(prev_hook);
		result
	}

	fn sys_math_u64(op: SyscallMathOp, a1: u64, a2: u64) -> u64 {
		syscall_math_op64(0, op.into(), a1, a2, 0, 0).unwrap()
	}

	fn sys_math_u32(op: SyscallMathOp, a1: u32, a2: u32) -> u64 {
		syscall_math_op64(0, op.into(), a1 as u64, a2 as u64, 0, 0).unwrap()
	}

	fn sys_math_f64(op: SyscallMathOp, a1: f64, a2: f64) -> f64 {
		f64::from_bits(syscall_math_op64(0, op.into(), a1.to_bits(), a2.to_bits(), 0, 0).unwrap())
	}

	fn sys_math_f64_u64(op: SyscallMathOp, a1: f64, a2: f64) -> u64 {
		syscall_math_op64(0, op.into(), a1.to_bits(), a2.to_bits(), 0, 0).unwrap()
	}

	fn sys_math_f32(op: SyscallMathOp, a1: f32, a2: f32) -> f32 {
		f32::from_bits(
			u32::try_from(
				syscall_math_op64(0, op.into(), a1.to_bits() as u64, a2.to_bits() as u64, 0, 0)
					.unwrap(),
			)
			.unwrap(),
		)
	}

	fn sys_math_f64_unary(op: SyscallMathOp, a1: f64) -> f64 {
		f64::from_bits(
			syscall_math_op64(0, op.into(), a1.to_bits(), 0.0f64.to_bits(), 0, 0).unwrap(),
		)
	}

	fn sys_math_f32_unary(op: SyscallMathOp, a1: f32) -> f32 {
		f32::from_bits(
			u32::try_from(
				syscall_math_op64(0, op.into(), a1.to_bits() as u64, 0.0f32.to_bits() as u64, 0, 0)
					.unwrap(),
			)
			.unwrap(),
		)
	}

	fn sys_math_f32_unary_u64(op: SyscallMathOp, a1: f32) -> u64 {
		syscall_math_op64(0, op.into(), a1.to_bits() as u64, 0, 0, 0).unwrap()
	}

	fn sys_math_f64_unary_u64(op: SyscallMathOp, a1: f64) -> u64 {
		syscall_math_op64(0, op.into(), a1.to_bits(), 0.0f64.to_bits(), 0, 0).unwrap()
	}

	fn sys_math_f32_unary_u32(op: SyscallMathOp, a1: f32) -> u32 {
		syscall_math_op64(0, op.into(), a1.to_bits() as u64, 0, 0, 0).unwrap() as u32
	}

	fn sys_math_f64_unary_u32(op: SyscallMathOp, a1: f64) -> u32 {
		syscall_math_op64(0, op.into(), a1.to_bits() as u64, 0, 0, 0).unwrap() as u32
	}

	#[test]
	fn test_i64div_s() {
		assert_eq!(3, sys_math_u64(SyscallMathOp::I64DivS, -6i64 as u64, -2i64 as u64));
		assert_eq!(-3, sys_math_u64(SyscallMathOp::I64DivS, -6i64 as u64, 2i64 as u64) as i64);
	}

	#[test]
	fn test_i32div_s() {
		assert_eq!(3, sys_math_u32(SyscallMathOp::I32DivS, -6i32 as u32, -2i32 as u32));
		assert_eq!(-3, sys_math_u32(SyscallMathOp::I32DivS, -6i32 as u32, 2i32 as u32) as i32);
	}

	#[test]
	fn division_by_zero() {
		assert!(syscall_math_op64(0, SyscallMathOp::I32DivS.into(), -6i64 as u64, 0 as u64, 0, 0).is_err());
		assert!(syscall_math_op64(0, SyscallMathOp::I64DivS.into(), -6i64 as u64, 0 as u64, 0, 0).is_err());
		assert!(syscall_math_op64(0, SyscallMathOp::I32RemS.into(), -6i64 as u64, 0 as u64, 0, 0).is_err());
		assert!(syscall_math_op64(0, SyscallMathOp::I64RemS.into(), -6i64 as u64, 0 as u64, 0, 0).is_err());

		assert_eq!(f32::INFINITY, sys_math_f32(SyscallMathOp::F32Div, 1.0f32, 0.0f32));
		assert_eq!(f32::INFINITY, sys_math_f32(SyscallMathOp::F32Div, -1.0f32, -0.0f32));
		assert_eq!(f32::NEG_INFINITY, sys_math_f32(SyscallMathOp::F32Div, 1.0f32, -0.0f32));
		assert_eq!(f32::NEG_INFINITY, sys_math_f32(SyscallMathOp::F32Div, -1.0f32, 0.0f32));

		assert_eq!(f64::INFINITY, sys_math_f64(SyscallMathOp::F64Div, 1.0, 0.0));
		assert_eq!(f64::INFINITY, sys_math_f64(SyscallMathOp::F64Div, -1.0, -0.0));
		assert_eq!(f64::NEG_INFINITY, sys_math_f64(SyscallMathOp::F64Div, 1.0, -0.0));
		assert_eq!(f64::NEG_INFINITY, sys_math_f64(SyscallMathOp::F64Div, 1.0, -0.0));

		assert!(sys_math_f32(SyscallMathOp::F32Div, f32::NAN, 0.0).is_nan());
		assert!(sys_math_f32(SyscallMathOp::F32Div, f32::NAN, -0.0).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::NAN, 0.0).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::NAN, -0.0).is_nan());
	}

	#[test]
	fn test_f64_convert_i64u() {
		assert_eq!(
			11f64,
			f64::from_bits(
				syscall_math_op64(0, SyscallMathOp::F64ConvertI64U.into(), 11u64, 0, 0, 0).unwrap()
			)
		);
		assert_eq!(
			0f64,
			f64::from_bits(
				syscall_math_op64(0, SyscallMathOp::F64ConvertI64U.into(), 0u64, 0, 0, 0).unwrap()
			)
		);

		let inf = 0x7FF0000000000000u64;
		let neg_inf = 0xFFF0000000000000u64;
		assert_eq!(
			9218868437227405000f64,
			f64::from_bits(
				syscall_math_op64(0, SyscallMathOp::F64ConvertI64U.into(), inf, 0, 0, 0).unwrap()
			)
		);
		assert_eq!(
			18442240474082181000f64,
			f64::from_bits(
				syscall_math_op64(0, SyscallMathOp::F64ConvertI64U.into(), neg_inf, 0, 0, 0).unwrap()
			)
		);

		let qnan = 0x7FF8000000000000u64;
		let snan = 0x7FF0000000000001u64;
		assert_eq!(
			9221120237041091000f64,
			f64::from_bits(
				syscall_math_op64(0, SyscallMathOp::F64ConvertI64U.into(), qnan, 0, 0, 0).unwrap()
			)
		);
		assert_eq!(
			9218868437227405000f64,
			f64::from_bits(
				syscall_math_op64(0, SyscallMathOp::F64ConvertI64U.into(), snan, 0, 0, 0).unwrap()
			)
		);

		assert_eq!(
			18446744073709552000f64,
			f64::from_bits(
				syscall_math_op64(0, SyscallMathOp::F64ConvertI64U.into(), u64::MAX, 0, 0, 0)
					.unwrap()
			)
		);
	}

	#[test]
	fn test_f64_add() {
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Add, 0.0, 0.0));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Add, 1.0, -1.0));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Add, -1.0, 1.0));
		assert_eq!(-2.0, sys_math_f64(SyscallMathOp::F64Add, -1.0, -1.0));
		assert_eq!(2.0, sys_math_f64(SyscallMathOp::F64Add, 1.0, 1.0));

		assert_eq!(1.0, sys_math_f64(SyscallMathOp::F64Add, 1.0, 0.0));
		assert_eq!(1.0, sys_math_f64(SyscallMathOp::F64Add, 0.0, 1.0));
		assert_eq!(-1.0, sys_math_f64(SyscallMathOp::F64Add, -1.0, 0.0));
		assert_eq!(-1.0, sys_math_f64(SyscallMathOp::F64Add, 0.0, -1.0));

		assert_eq!(f64::MAX, sys_math_f64(SyscallMathOp::F64Add, f64::MAX, 1.0));
		assert_eq!(f64::INFINITY, sys_math_f64(SyscallMathOp::F64Add, f64::MAX, f64::MAX));

		assert_eq!(
			f64::INFINITY,
			sys_math_f64(SyscallMathOp::F64Add, f64::INFINITY, f64::INFINITY)
		);
		assert_eq!(
			f64::NEG_INFINITY,
			sys_math_f64(SyscallMathOp::F64Add, f64::NEG_INFINITY, f64::NEG_INFINITY)
		);
		assert!(sys_math_f64(SyscallMathOp::F64Add, f64::INFINITY, f64::NEG_INFINITY).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Add, f64::NEG_INFINITY, f64::INFINITY).is_nan());
	}

	#[test]
	fn test_f64_sub() {
		assert_eq!(1.0, sys_math_f64(SyscallMathOp::F64Sub, 2.5, 1.5));
		assert_eq!(-1.0, sys_math_f64(SyscallMathOp::F64Sub, 1.5, 2.5));

		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Sub, 1.5, 1.5));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Sub, 0.0, 0.0));

		assert_eq!(f64::INFINITY, sys_math_f64(SyscallMathOp::F64Sub, f64::INFINITY, 1.5));
		assert_eq!(f64::INFINITY, sys_math_f64(SyscallMathOp::F64Sub, f64::INFINITY, -1.5));
		assert!(sys_math_f64(SyscallMathOp::F64Sub, f64::INFINITY, f64::INFINITY).is_nan());
		assert_eq!(
			f64::INFINITY,
			sys_math_f64(SyscallMathOp::F64Sub, f64::INFINITY, f64::NEG_INFINITY)
		);
		assert!(sys_math_f64(SyscallMathOp::F64Sub, f64::INFINITY, f64::NAN).is_nan());

		assert_eq!(f64::NEG_INFINITY, sys_math_f64(SyscallMathOp::F64Sub, f64::NEG_INFINITY, 1.5));
		assert_eq!(f64::NEG_INFINITY, sys_math_f64(SyscallMathOp::F64Sub, f64::NEG_INFINITY, -1.5));
		assert_eq!(
			f64::NEG_INFINITY,
			sys_math_f64(SyscallMathOp::F64Sub, f64::NEG_INFINITY, f64::INFINITY)
		);
		assert!(sys_math_f64(SyscallMathOp::F64Sub, f64::NEG_INFINITY, f64::NEG_INFINITY).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Sub, f64::NEG_INFINITY, f64::NAN).is_nan());

		assert!(sys_math_f64(SyscallMathOp::F64Sub, f64::NAN, 1.5).is_nan());
	}

	#[test]
	fn test_f64_mul() {
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Mul, 0.0f64, 0.0f64));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Mul, 3.0, 0.0));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Mul, 3.0, -0.0));
		assert_eq!(2.0, sys_math_f64(SyscallMathOp::F64Mul, -2.0, -1.0));

		assert_eq!(12.0, sys_math_f64(SyscallMathOp::F64Mul, 3.0, 4.0));
		assert_eq!(12.0, sys_math_f64(SyscallMathOp::F64Mul, 4.0, 3.0));

		assert_eq!(f64::MAX, sys_math_f64(SyscallMathOp::F64Mul, f64::MAX, 1.0));
		assert_eq!(f64::INFINITY, sys_math_f64(SyscallMathOp::F64Mul, f64::MAX, f64::MAX));

		assert_eq!(
			f64::INFINITY,
			sys_math_f64(SyscallMathOp::F64Mul, f64::INFINITY, f64::INFINITY)
		);
		assert_eq!(
			f64::INFINITY,
			sys_math_f64(SyscallMathOp::F64Mul, f64::NEG_INFINITY, f64::NEG_INFINITY)
		);
		assert_eq!(
			f64::NEG_INFINITY,
			sys_math_f64(SyscallMathOp::F64Mul, f64::NEG_INFINITY, f64::INFINITY)
		);
		assert_eq!(
			f64::NEG_INFINITY,
			sys_math_f64(SyscallMathOp::F64Mul, f64::INFINITY, f64::NEG_INFINITY)
		);

		assert!(sys_math_f64(SyscallMathOp::F64Mul, f64::INFINITY, f64::NAN).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Mul, f64::NAN, f64::INFINITY).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Mul, f64::NEG_INFINITY, f64::NAN).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Mul, f64::NAN, f64::NEG_INFINITY).is_nan());
	}

	#[test]
	fn test_f64_div() {
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Div, 0.0, 1.0));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Div, -0.0, 1.0));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Div, -0.0, 1.0));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Div, -0.0, f64::INFINITY));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Div, 0.0, f64::NEG_INFINITY));
		assert!(sys_math_f64(SyscallMathOp::F64Div, 0.0, f64::NAN).is_nan());

		assert_eq!(1.5, sys_math_f64(SyscallMathOp::F64Div, -3.0, -2.0));
		assert_eq!(-1.5, sys_math_f64(SyscallMathOp::F64Div, 3.0, -2.0));
		assert_eq!(-1.5, sys_math_f64(SyscallMathOp::F64Div, -3.0, 2.0));

		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Div, -3.0, f64::INFINITY));
		assert_eq!(0.0, sys_math_f64(SyscallMathOp::F64Div, 3.0, f64::NEG_INFINITY));
		assert!(sys_math_f64(SyscallMathOp::F64Div, 3.0, f64::NAN).is_nan());

		assert_eq!(f64::MAX, sys_math_f64(SyscallMathOp::F64Div, f64::MAX, 1.0));
		assert_eq!(f64::INFINITY, sys_math_f64(SyscallMathOp::F64Div, f64::INFINITY, f64::MAX));
		assert_eq!(1.0, sys_math_f64(SyscallMathOp::F64Div, f64::MAX, f64::MAX));

		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::INFINITY, f64::INFINITY).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::NEG_INFINITY, f64::NEG_INFINITY).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::NEG_INFINITY, f64::INFINITY).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::INFINITY, f64::NEG_INFINITY).is_nan());

		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::INFINITY, f64::NAN).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::NAN, f64::INFINITY).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::NEG_INFINITY, f64::NAN).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::NAN, f64::NEG_INFINITY).is_nan());
	}

	#[test]
	fn test_f64_eq() {
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Eq, 0.0, 0.0));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Eq, 0.0, -0.0));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Eq, 5.0, 5.0));

		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Eq, f64::MAX, f64::MAX));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Eq, f64::MIN, f64::MIN));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Eq, f64::INFINITY, f64::INFINITY));

		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Eq, f64::INFINITY, f64::NEG_INFINITY));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Eq, f64::NEG_INFINITY, f64::INFINITY));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Eq, f64::NAN, f64::NAN));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Eq, 1.0, f64::NAN));
	}

	#[test]
	fn test_f64_ne() {
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ne, 0.0, 0.0));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ne, 5.0, 5.0));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ne, 0.0, -0.0));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ne, f64::MAX, f64::MAX));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ne, f64::MIN, f64::MIN));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ne, f64::INFINITY, f64::INFINITY));

		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ne, 5.0, 3.0));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ne, f64::INFINITY, f64::NEG_INFINITY));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ne, f64::NEG_INFINITY, f64::INFINITY));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ne, f64::NAN, f64::NAN));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ne, 1.0, f64::NAN));
	}

	#[test]
	fn test_f64_neg() {
		assert_eq!(-0.0, sys_math_f64_unary(SyscallMathOp::F64Neg, 0.0));
		assert_eq!(-5.0, sys_math_f64_unary(SyscallMathOp::F64Neg, 5.0));
		assert_eq!(5.0, sys_math_f64_unary(SyscallMathOp::F64Neg, -5.0));
		assert_eq!(0.0, sys_math_f64_unary(SyscallMathOp::F64Neg, -0.0));

		assert_eq!(-f64::MAX, sys_math_f64_unary(SyscallMathOp::F64Neg, f64::MAX));
		assert_eq!(-f64::MIN, sys_math_f64_unary(SyscallMathOp::F64Neg, f64::MIN));
		assert_eq!(f64::MAX, sys_math_f64_unary(SyscallMathOp::F64Neg, -f64::MAX));
		assert_eq!(f64::MIN, sys_math_f64_unary(SyscallMathOp::F64Neg, -f64::MIN));
		assert_eq!(f64::NEG_INFINITY, sys_math_f64_unary(SyscallMathOp::F64Neg, f64::INFINITY));
		assert_eq!(f64::INFINITY, sys_math_f64_unary(SyscallMathOp::F64Neg, f64::NEG_INFINITY));

		assert!(sys_math_f64_unary(SyscallMathOp::F64Neg, f64::NAN).is_nan());
	}

	#[test]
	fn test_f64_ge() {
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, -0.0, 0.0));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, 0.0, -0.0));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, f64::MAX, f64::MAX));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, f64::MIN, f64::MIN));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, f64::INFINITY, f64::INFINITY));

		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, 5.0, 5.0));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, 5.0, 3.0));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ge, 3.0, 5.0));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ge, -3.0, 5.0));
		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, 3.0, -5.0));

		assert_eq!(1, sys_math_f64_u64(SyscallMathOp::F64Ge, f64::INFINITY, f64::NEG_INFINITY));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ge, f64::NEG_INFINITY, f64::INFINITY));

		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ge, f64::NAN, f64::NAN));
		assert_eq!(0, sys_math_f64_u64(SyscallMathOp::F64Ge, 1.0, f64::NAN));
	}

	#[test]
	fn test_float_abs() {
		// F64
		assert_eq!(6.123f64, sys_math_f64_unary(SyscallMathOp::F64Abs, -6.123f64));
		assert_eq!(6.123f64, sys_math_f64_unary(SyscallMathOp::F64Abs, 6.123f64));
		assert_eq!(f64::INFINITY, sys_math_f64_unary(SyscallMathOp::F64Abs, f64::NEG_INFINITY));
		assert_eq!(f64::INFINITY, sys_math_f64_unary(SyscallMathOp::F64Abs, f64::INFINITY));
		assert!(sys_math_f64_unary(SyscallMathOp::F64Abs, -f64::NAN).is_nan());
		assert!(sys_math_f64_unary(SyscallMathOp::F64Abs, f64::NAN).is_nan());

		// F32
		assert_eq!(6.123f32, sys_math_f32_unary(SyscallMathOp::F32Abs, -6.123f32));
		assert_eq!(6.123f32, sys_math_f32_unary(SyscallMathOp::F32Abs, 6.123f32));
		assert_eq!(f32::INFINITY, sys_math_f32_unary(SyscallMathOp::F32Abs, -f32::INFINITY));
		assert_eq!(f32::INFINITY, sys_math_f32_unary(SyscallMathOp::F32Abs, f32::INFINITY));
		assert!(sys_math_f32_unary(SyscallMathOp::F32Abs, -f32::NAN).is_nan());
		assert!(sys_math_f32_unary(SyscallMathOp::F32Abs, f32::NAN).is_nan());
	}

	#[test]
	fn test_float_trunc() {
		// F64
		assert_eq!(0i64, sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64S, 0.0) as i64);
		assert_eq!(
			i64::MIN,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64S, i64::MIN.to_f64().unwrap()) as i64
		);
		// maximum accepted value of f64 is 9223372036854775295.9 which can be to 9223372036854774784 in integer type
		assert_eq!(
			9223372036854774784,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64S, 9223372036854774784.0) as i64
		);
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64S, f64::INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64S, f64::NEG_INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64S, f64::NAN)).is_err());
		// out of range
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64S, f64::MIN)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64S, f64::MAX)).is_err());

		assert_eq!(0u64, sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64U, 0.0));
		assert_eq!(
			u64::MIN,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64U, u64::MIN.to_f64().unwrap())
		);
		// maximum accepted value of f64 is 9223372036854775295.9 which can be to 9223372036854774784 in integer type
		assert_eq!(
			9223372036854774784,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64U, 9223372036854774784.0)
		);
		// out of range
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64U, f64::MIN)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64U, f64::MAX)).is_err());

		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64U, f64::INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64U, f64::NEG_INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u64(SyscallMathOp::I64TruncF64U, f64::NAN)).is_err());

		// I32TruncF32S
		assert_eq!(0i32, sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32S, 0.0) as i32);
		assert_eq!(
			i32::MIN,
			sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32S, i32::MIN.to_f32().unwrap()) as i32
		);
		// maximum accepted value of f32 is 2147483583.9 which can be to 2147483520 in integer type
		assert_eq!(
			2147483520,
			sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32S, 2147483520.0) as i32
		);
		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32S, f32::INFINITY)).is_err());
		// out of range
		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32S, f32::MIN)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32S, f32::MAX)).is_err());

		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32S, f32::NEG_INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32S, f32::NAN)).is_err());

		// I32TruncF32U
		assert_eq!(0u32, sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32U, 0.0));
		assert_eq!(
			u32::MIN,
			sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32U, u32::MIN.to_f32().unwrap())
		);
		// maximum accepted value of f32 is 2147483583.9 which can be to 2147483520 in integer type
		assert_eq!(2147483520, sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32U, 2147483520.0));
		// out of range
		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32U, f32::MIN)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32U, f32::MAX)).is_err());

		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32U, f32::INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32U, f32::NEG_INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u32(SyscallMathOp::I32TruncF32U, f32::NAN)).is_err());

		// I32TruncF64S
		assert_eq!(0i32, sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64S, 0.0) as i32);
		assert_eq!(
			i32::MIN,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64S, i32::MIN.to_f64().unwrap()) as i32
		);
		// maximum accepted value of f32 is 2147483583.9 which can be to 2147483520 in integer type
		assert_eq!(
			2147483520,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64S, 2147483520.0) as i32
		);
		// out of range
		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64S, f64::MIN)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64S, f64::MAX)).is_err());

		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64S, f64::INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64S, f64::NEG_INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64S, f64::NAN)).is_err());

		// I32TruncF64U
		assert_eq!(0u32, sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64U, 0.0));
		assert_eq!(
			u32::MIN,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64U, u32::MIN.to_f64().unwrap())
		);
		// maximum accepted value of f32 is 2147483583.9 which can be to 2147483520 in integer type
		assert_eq!(
			2147483520,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64U, 2147483520.0) as i32
		);
		// out of range
		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64U, f64::MIN)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64U, f64::MAX)).is_err());

		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64U, f64::INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64U, f64::NEG_INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f64_unary_u32(SyscallMathOp::I32TruncF64U, f64::NAN)).is_err());

		// I64TruncF32S
		assert_eq!(0i64, sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32S, 0.0) as i64);
		assert_eq!(
			i64::MIN,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32S, i64::MIN.to_f32().unwrap()) as i64
		);
		// maximum accepted value of f32 is 9223370999999999999.9 which can be to 9223370937343148032 in integer type
		assert_eq!(
			9223370937343148032,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32S, 9223370937343148032.0) as i64
		);
		// out of range
		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32S, f32::MIN)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32S, f32::MAX)).is_err());

		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32S, f32::INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32S, f32::NEG_INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32S, f32::NAN)).is_err());

		// I64TruncF32U
		assert_eq!(0u64, sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32U, 0.0));
		assert_eq!(
			u64::MIN,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32U, u64::MIN.to_f32().unwrap())
		);
		// maximum accepted value of f32 is 9223370999999999999.9 which can be to 9223370937343148032 in integer type
		assert_eq!(
			9223370937343148032,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32U, 9223370937343148032.0)
		);
		// out of range
		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32U, f32::MIN)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32U, f32::MAX)).is_err());

		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32U, f32::INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32U, f32::NEG_INFINITY)).is_err());
		assert!(catch_panic(|| sys_math_f32_unary_u64(SyscallMathOp::I64TruncF32U, f32::NAN)).is_err());

		// I32TruncSatF32S
		assert_eq!(0i32, sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32S, 0.0) as i32);
		assert_eq!(
			i32::MIN,
			sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32S, f32::NEG_INFINITY) as i32
		);
		assert_eq!(
			i32::MAX,
			sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32S, f32::INFINITY) as i32
		);
		assert_eq!(0i32, sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32S, f32::NAN) as i32);
		// out of range
		assert_eq!(
			i32::MIN,
			sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32S, f32::MIN) as i32
		);
		assert_eq!(
			i32::MAX,
			sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32S, f32::MAX) as i32
		);

		// I32TruncSatF32U
		assert_eq!(0i32, sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32U, 0.0) as i32);
		assert_eq!(
			u32::MIN,
			sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32U, f32::NEG_INFINITY)
		);
		assert_eq!(u32::MAX, sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32U, f32::INFINITY));
		assert_eq!(0u32, sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32U, f32::NAN));
		// out of range
		assert_eq!(u32::MIN, sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32U, f32::MIN));
		assert_eq!(u32::MAX, sys_math_f32_unary_u32(SyscallMathOp::I32TruncSatF32U, f32::MAX));

		// I32TruncSatF64S
		assert_eq!(0i32, sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64S, 0.0) as i32);
		assert_eq!(
			i32::MIN,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64S, f64::NEG_INFINITY) as i32
		);
		assert_eq!(
			i32::MAX,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64S, f64::INFINITY) as i32
		);
		assert_eq!(0i32, sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64S, f64::NAN) as i32);
		// out of range
		assert_eq!(
			i32::MIN,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64S, f64::MIN) as i32
		);
		assert_eq!(
			i32::MAX,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64S, f64::MAX) as i32
		);

		// I32TruncSatF64U
		assert_eq!(0i32, sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64U, 0.0) as i32);
		assert_eq!(
			u32::MIN,
			sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64U, f64::NEG_INFINITY)
		);
		assert_eq!(u32::MAX, sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64U, f64::INFINITY));
		assert_eq!(0u32, sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64U, f64::NAN));
		// out of range
		assert_eq!(u32::MIN, sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64U, f64::MIN));
		assert_eq!(u32::MAX, sys_math_f64_unary_u32(SyscallMathOp::I32TruncSatF64U, f64::MAX));

		// I64TruncSatF32S
		assert_eq!(0i64, sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32S, 0.0) as i64);
		assert_eq!(
			i64::MIN,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32S, f32::NEG_INFINITY) as i64
		);
		assert_eq!(
			i64::MAX,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32S, f32::INFINITY) as i64
		);
		assert_eq!(0i64, sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32S, f32::NAN) as i64);
		// out of range
		assert_eq!(
			i64::MIN,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32S, f32::MIN) as i64
		);
		assert_eq!(
			i64::MAX,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32S, f32::MAX) as i64
		);

		// I64TruncSatF32U
		assert_eq!(0u64, sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32U, 0.0));
		assert_eq!(
			u64::MIN,
			sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32U, f32::NEG_INFINITY)
		);
		assert_eq!(u64::MAX, sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32U, f32::INFINITY));
		assert_eq!(0u64, sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32U, f32::NAN));
		// out of range
		assert_eq!(u64::MIN, sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32U, f32::MIN));
		assert_eq!(u64::MAX, sys_math_f32_unary_u64(SyscallMathOp::I64TruncSatF32U, f32::MAX));

		// I64TruncSatF64S
		assert_eq!(0i64, sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64S, 0.0) as i64);
		assert_eq!(
			i64::MIN,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64S, f64::NEG_INFINITY) as i64
		);
		assert_eq!(
			i64::MAX,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64S, f64::INFINITY) as i64
		);
		assert_eq!(0i64, sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64S, f64::NAN) as i64);
		// out of range
		assert_eq!(
			i64::MIN,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64S, f64::MIN) as i64
		);
		assert_eq!(
			i64::MAX,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64S, f64::MAX) as i64
		);

		// I64TruncSatF64U
		assert_eq!(0u64, sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64U, 0.0));
		assert_eq!(
			u64::MIN,
			sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64U, f64::NEG_INFINITY)
		);
		assert_eq!(u64::MAX, sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64U, f64::INFINITY));
		assert_eq!(0u64, sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64U, f64::NAN));
		// out of range
		assert_eq!(u64::MIN, sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64U, f64::MIN));
		assert_eq!(u64::MAX, sys_math_f64_unary_u64(SyscallMathOp::I64TruncSatF64U, f64::MAX));
	}

	#[test]
	fn test_f64_nan_handling() {
		assert!(sys_math_f64(SyscallMathOp::F64Add, f64::NAN, 1.0).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Sub, 1.0, f64::NAN).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Mul, f64::NAN, f64::NAN).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::NAN, 1.0).is_nan());
		assert_eq!(sys_math_f64_u64(SyscallMathOp::F64Eq, f64::NAN, f64::NAN), 0);
		assert_eq!(sys_math_f64_u64(SyscallMathOp::F64Ne, f64::NAN, 1.0), 1);
	}

	#[test]
	fn test_f64_edge_cases() {
		// Addition edge cases
		assert_eq!(sys_math_f64(SyscallMathOp::F64Add, f64::INFINITY, 1.0), f64::INFINITY, "Adding 1.0 to INFINITY should yield INFINITY");
		assert_eq!(sys_math_f64(SyscallMathOp::F64Add, f64::MAX, 0.0), f64::MAX, "Adding 0.0 to MAX should yield MAX");
		assert_eq!(sys_math_f64(SyscallMathOp::F64Add, f64::MAX, f64::MIN), f64::MAX + f64::MIN, "Adding f64::MAX with f64::MIN");
		assert!(sys_math_f64(SyscallMathOp::F64Add, f64::INFINITY, f64::NEG_INFINITY).is_nan(), "Adding INFINITY with NEG_INFINITY should yield NaN");

		// Subtraction edge cases
		assert_eq!(sys_math_f64(SyscallMathOp::F64Sub, f64::NEG_INFINITY, 1.0), f64::NEG_INFINITY, "Subtracting 1.0 from NEG_INFINITY should yield NEG_INFINITY");
		assert_eq!(sys_math_f64(SyscallMathOp::F64Sub, f64::MIN, 0.0), f64::MIN, "Subtracting 0.0 from MIN should yield MIN");
		assert_eq!(sys_math_f64(SyscallMathOp::F64Sub, f64::MAX, f64::MIN), f64::MAX - f64::MIN, "Subtracting f64::MIN from f64::MAX");
		assert!(sys_math_f64(SyscallMathOp::F64Sub, f64::INFINITY, f64::INFINITY).is_nan(), "Subtracting INFINITY from INFINITY should yield NaN");

		// Multiplication edge cases
		assert!(sys_math_f64(SyscallMathOp::F64Mul, 0.0, f64::INFINITY).is_nan(), "Multiplying 0.0 by INFINITY should yield NaN");
		assert_eq!(sys_math_f64(SyscallMathOp::F64Mul, f64::MAX, f64::MIN), f64::MAX * f64::MIN, "Multiplying f64::MAX with f64::MIN should yield a finite result");
		assert_eq!(sys_math_f64(SyscallMathOp::F64Mul, f64::INFINITY, f64::NEG_INFINITY), f64::NEG_INFINITY, "Multiplying INFINITY with NEG_INFINITY should yield NEG_INFINITY");
		// Division edge cases
		assert!(sys_math_f64(SyscallMathOp::F64Div, 0.0, 0.0).is_nan(), "Dividing 0.0 by 0.0 should yield NaN");
		assert_eq!(sys_math_f64(SyscallMathOp::F64Div, f64::MAX, 0.0), f64::INFINITY, "Dividing f64::MAX by 0.0 should yield INFINITY");
		assert_eq!(sys_math_f64(SyscallMathOp::F64Div, f64::MAX, f64::MIN), f64::MAX / f64::MIN, "Dividing f64::MAX by f64::MIN");
		assert!(sys_math_f64(SyscallMathOp::F64Div, f64::INFINITY, f64::INFINITY).is_nan(), "Dividing INFINITY by INFINITY should yield NaN");
	}

	#[test]
	fn test_f32_edge_cases() {
		assert_eq!(sys_math_f32(SyscallMathOp::F32Add, f32::INFINITY, 1.0), f32::INFINITY, "Adding 1.0 to INFINITY should yield INFINITY");
		assert_eq!(sys_math_f32(SyscallMathOp::F32Add, f32::MAX, 0.0), f32::MAX, "Adding 0.0 to MAX should yield MAX");
		assert_eq!(sys_math_f32(SyscallMathOp::F32Sub, f32::NEG_INFINITY, 1.0), f32::NEG_INFINITY, "Subtracting 1.0 from NEG_INFINITY should yield NEG_INFINITY");
		assert_eq!(sys_math_f32(SyscallMathOp::F32Sub, f32::MIN, 0.0), f32::MIN, "Subtracting 0.0 from MIN should yield MIN");
		assert!(sys_math_f32(SyscallMathOp::F32Mul, 0.0, f32::INFINITY).is_nan(), "Multiplying 0.0 by INFINITY should yield NaN");
		assert!(sys_math_f32(SyscallMathOp::F32Div, 0.0, 0.0).is_nan(), "Dividing 0.0 by 0.0 should yield NaN");
		assert_eq!(sys_math_f32(SyscallMathOp::F32Add, f32::MAX, 0.0), f32::MAX, "Adding 0.0 to MAX should yield MAX");
		assert_eq!(sys_math_f32(SyscallMathOp::F32Sub, f32::MIN, 0.0), f32::MIN, "Subtracting 0.0 from MIN should yield MIN");
	}

	#[test]
	fn test_f64_boundary_values() {
		assert_eq!(sys_math_f64(SyscallMathOp::F64Add, f64::MIN, f64::MIN), f64::MIN + f64::MIN);
		assert_eq!(sys_math_f64(SyscallMathOp::F64Sub, f64::MAX, f64::MIN), f64::MAX - f64::MIN);
		assert_eq!(sys_math_f64(SyscallMathOp::F64Mul, f64::MIN, 2.0), f64::MIN * 2.0);
		assert_eq!(sys_math_f64(SyscallMathOp::F64Div, f64::MIN, 2.0), f64::MIN / 2.0);
	}

	#[test]
	fn test_f64_invalid_operations() {
		assert!(sys_math_f64(SyscallMathOp::F64Div, 1.0, 0.0).is_infinite());
		assert!(sys_math_f64(SyscallMathOp::F64Div, 0.0, 0.0).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Add, f64::NAN, f64::INFINITY).is_nan());
		assert!(sys_math_f64(SyscallMathOp::F64Sub, f64::INFINITY, f64::NAN).is_nan());
	}

	#[test]
	fn test_f64_rounding_behaviors() {
		let epsilon = 1e-10;
		assert!((sys_math_f64(SyscallMathOp::F64Add, 0.1, 0.2) - 0.3).abs() < epsilon, "Adding 0.1 and 0.2 should yield approximately 0.3");
		assert!((sys_math_f64(SyscallMathOp::F64Sub, 0.3, 0.1) - 0.2).abs() < epsilon, "Subtracting 0.1 from 0.3 should yield approximately 0.2");
		assert!((sys_math_f64(SyscallMathOp::F64Mul, 0.1, 0.2) - 0.02).abs() < epsilon, "Multiplying 0.1 and 0.2 should yield approximately 0.02");
		assert!((sys_math_f64(SyscallMathOp::F64Div, 0.3, 0.1) - 3.0).abs() < epsilon, "Dividing 0.3 by 0.1 should yield approximately 3.0");
	}

	#[test]
	fn test_f64_overflow_underflow() {
		assert_eq!(sys_math_f64(SyscallMathOp::F64Add, f64::MAX, f64::MAX), f64::INFINITY);
		assert_eq!(sys_math_f64(SyscallMathOp::F64Sub, f64::MIN, f64::MAX), -f64::INFINITY);
		assert_eq!(sys_math_f64(SyscallMathOp::F64Mul, f64::MAX, 2.0), f64::INFINITY);
		assert_eq!(sys_math_f64(SyscallMathOp::F64Div, f64::MIN, 2.0), f64::MIN / 2.0);
	}
}
