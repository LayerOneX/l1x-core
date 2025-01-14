use anyhow::{anyhow, Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i8)]
pub enum AccessType {
	PRIVATE = 0,
	PUBLIC = 1,
	RESTICTED = 2, /* Will be used in future to restrict the contract to be initiated by only
	                * specified addresses. */
}

impl TryInto<AccessType> for i8 {
	type Error = Error;

	fn try_into(self) -> Result<AccessType, Self::Error> {
		match self {
			0 => Ok(AccessType::PRIVATE),
			1 => Ok(AccessType::PUBLIC),
			2 => Ok(AccessType::RESTICTED),
			_ => Err(anyhow!("Invalid access type {}", self)),
		}
	}
}
