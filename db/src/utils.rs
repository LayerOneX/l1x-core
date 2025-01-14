use byteorder::{ByteOrder, LittleEndian};

/// Converts a value to a byte array in little endian format.
pub trait ToByteArray<T> {
	fn to_byte_array_le(&self, bytes: T) -> T
	where
		T: AsMut<[u8]>,
		T: Default;
}

/// Converts byte array to a value in little endian format.
pub trait FromByteArray<T> {
	fn from_byte_array(bytes: &[u8]) -> T;
}

//  Converters for u128 to byte array
impl<T> ToByteArray<T> for u128 {
	fn to_byte_array_le(&self, bytes: T) -> T
	where
		T: AsMut<[u8]>,
		T: Default,
	{
		let mut bytes = bytes;
		LittleEndian::write_u128(bytes.as_mut(), *self);
		bytes
	}
}

//  Converters for u64 to byte array
impl<T> ToByteArray<T> for u64 {
	fn to_byte_array_le(&self, bytes: T) -> T
	where
		T: AsMut<[u8]>,
		T: Default,
	{
		let mut bytes = bytes;
		LittleEndian::write_u64(bytes.as_mut(), *self);
		bytes
	}
}

//  Converters for u32 to byte array
impl<T> ToByteArray<T> for u32 {
	fn to_byte_array_le(&self, bytes: T) -> T
	where
		T: AsMut<[u8]>,
		T: Default,
	{
		let mut bytes = bytes;
		LittleEndian::write_u32(bytes.as_mut(), *self);
		bytes
	}
}

// Converters for u128 from byte array
// Usage: let value: u128 = u128::from_byte_array(&bytes);
impl FromByteArray<u128> for u128 {
	fn from_byte_array(bytes: &[u8]) -> u128 {
		LittleEndian::read_u128(bytes)
	}
}

impl FromByteArray<u64> for u64 {
	fn from_byte_array(bytes: &[u8]) -> u64 {
		LittleEndian::read_u64(bytes)
	}
}

pub mod big_int {
	use num_bigint::BigInt;
	use num_traits::ToPrimitive;

	/// Converts a number to a big integer.
	pub trait ToBigInt {
		fn get_big_int(&self) -> BigInt;
	}

	/// Converts a big integer to a number.
	pub trait FromBigInt {
		fn from_big_int(big_int: &BigInt) -> Self;
	}

	impl<T> ToBigInt for T
	where
		T: ToPrimitive,
	{
		fn get_big_int(&self) -> BigInt {
			if let Some(n) = self.to_u128() {
				BigInt::from(n)
			} else {
				panic!("Failed to convert to BigInt");
			}
		}
	}

	impl FromBigInt for u128 {
		fn from_big_int(big_int: &BigInt) -> Self {
			big_int.to_u128().unwrap()
		}
	}

	impl FromBigInt for u64 {
		fn from_big_int(big_int: &BigInt) -> Self {
			big_int.to_u64().unwrap()
		}
	}

	impl FromBigInt for u32 {
		fn from_big_int(big_int: &BigInt) -> Self {
			big_int.to_u32().unwrap()
		}
	}
}
