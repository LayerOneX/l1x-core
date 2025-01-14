use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use compile_time_config::PROTOCOL_VERSION;


#[derive(Serialize, Debug, Clone)]
pub struct BoundedVersion<const VERSION: u32>(u32);

impl<const VERSION: u32> BoundedVersion<VERSION> {
	pub fn new(x: u32) -> Result<Self, anyhow::Error> {
		if x != VERSION {
			Err(anyhow!("Expected Message version is {}, the actual version is {}", VERSION, x))
		} else {
			Ok(Self(x))
		}
	}
}

impl<const VERSION: u32> Default for BoundedVersion<VERSION> {
	fn default() -> Self {
		Self(VERSION)
	}
}

impl<'de, const VERSION: u32> Deserialize<'de> for BoundedVersion<VERSION> {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		// Or `Self::new` and return an explicit error
		match Deserialize::deserialize(deserializer).map(Self::new) {
			Ok(ret) => ret.map_err(|e| serde::de::Error::custom(e.to_string())),
			Err(e) => Err(e),
		}
	}
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VersionedMessage<const VERSION: u32, T> {
	pub protocol_version: BoundedVersion<VERSION>,
	pub data: T,
}

impl<const VERSION: u32, T> VersionedMessage<VERSION, T> {
	pub fn new(data: T) -> Self {
		Self { protocol_version: BoundedVersion::<VERSION>::default(), data }
	}
}

pub fn serialize_as_versioned_message<T>(data: T) -> Result<Vec<u8>, anyhow::Error>
where
	T: serde::Serialize,
{
	let versioned = VersionedMessage::<PROTOCOL_VERSION, T>::new(data);
	Ok(bincode::serialize(&versioned)?)
}

pub fn deserialize_from_versioned_message<'a, 'b, T>(data: &'a Vec<u8>) -> Result<T, anyhow::Error>
where
	T: serde::Deserialize<'b>,
	'a: 'b,
{
	let versioned = bincode::deserialize::<VersionedMessage<PROTOCOL_VERSION, T>>(data)?;
	Ok(versioned.data)
}
