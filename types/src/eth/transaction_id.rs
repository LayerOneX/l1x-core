use ethereum_types::U64;
use serde_repr::*;

#[derive(Serialize_repr, Eq, Hash, Deserialize_repr, Debug, Copy, Clone, PartialEq, Default)]
#[repr(u8)]
pub enum TypedTxId {
	EIP1559Transaction = 0x02,
	AccessList = 0x01,
	#[default]
	Legacy = 0x00,
}

impl TypedTxId {
	// used in json tets
	pub fn from_u8_id(n: u8) -> Option<Self> {
		match n {
			0 => Some(Self::Legacy),
			1 => Some(Self::AccessList),
			2 => Some(Self::EIP1559Transaction),
			_ => None,
		}
	}

	pub fn try_from_wire_byte(n: u8) -> Result<Self, ()> {
		match n {
			x if x == TypedTxId::EIP1559Transaction as u8 => Ok(TypedTxId::EIP1559Transaction),
			x if x == TypedTxId::AccessList as u8 => Ok(TypedTxId::AccessList),
			x if (x & 0x80) != 0x00 => Ok(TypedTxId::Legacy),
			_ => Err(()),
		}
	}

	#[allow(non_snake_case)]
	pub fn from_U64_option_id(n: Option<U64>) -> Option<Self> {
		match n.map(|t| t.as_u64()) {
			None => Some(Self::Legacy),
			Some(0x00) => Some(Self::Legacy),
			Some(0x01) => Some(Self::AccessList),
			Some(0x02) => Some(Self::EIP1559Transaction),
			_ => None,
		}
	}

	#[allow(non_snake_case)]
	pub fn to_U64_option_id(self) -> Option<U64> {
		Some(U64::from(self as u8))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn typed_tx_id_try_from_wire() {
		assert_eq!(Ok(TypedTxId::EIP1559Transaction), TypedTxId::try_from_wire_byte(0x02));
		assert_eq!(Ok(TypedTxId::AccessList), TypedTxId::try_from_wire_byte(0x01));
		assert_eq!(Ok(TypedTxId::Legacy), TypedTxId::try_from_wire_byte(0x81));
		assert_eq!(Err(()), TypedTxId::try_from_wire_byte(0x00));
		assert_eq!(Err(()), TypedTxId::try_from_wire_byte(0x03));
	}

	#[test]
	fn typed_tx_id_to_u64_option_id() {
		assert_eq!(Some(U64::from(0x00)), TypedTxId::Legacy.to_U64_option_id());
		assert_eq!(Some(U64::from(0x01)), TypedTxId::AccessList.to_U64_option_id());
		assert_eq!(Some(U64::from(0x02)), TypedTxId::EIP1559Transaction.to_U64_option_id());
	}

	#[test]
	fn typed_tx_id_from_u64_option_id() {
		assert_eq!(Some(TypedTxId::Legacy), TypedTxId::from_U64_option_id(None));
		assert_eq!(
			Some(TypedTxId::AccessList),
			TypedTxId::from_U64_option_id(Some(U64::from(0x01)))
		);
		assert_eq!(
			Some(TypedTxId::EIP1559Transaction),
			TypedTxId::from_U64_option_id(Some(U64::from(0x02)))
		);
		assert_eq!(None, TypedTxId::from_U64_option_id(Some(U64::from(0x03))));
	}

	#[test]
	fn typed_tx_id_from_u8_id() {
		assert_eq!(Some(TypedTxId::Legacy), TypedTxId::from_u8_id(0));
		assert_eq!(Some(TypedTxId::AccessList), TypedTxId::from_u8_id(1));
		assert_eq!(Some(TypedTxId::EIP1559Transaction), TypedTxId::from_u8_id(2));
		assert_eq!(None, TypedTxId::from_u8_id(3));
	}
}
