use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use compile_time_config::PROTOCOL_VERSION;
// use log::debug;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Bincode deserialization/serialization error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("Network namespace mismatch: Expected {expected}, got {received}")]
    NamespaceMismatch { expected: String, received: String },

    // Modify this variant
    #[error("Unsupported protocol version: Expected {expected}, got {received}")]
    VersionMismatch { expected: u32, received: u32 },
}

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
	// pub network_namespace_id: String,
	pub data: T,
}

impl<const VERSION: u32, T> VersionedMessage<VERSION, T> {
	pub fn new(data: T) -> Self {
		// Self { protocol_version: BoundedVersion::<VERSION>::default(), network_namespace_id: network_namespace_id.to_string(), data }
		Self { protocol_version: BoundedVersion::<VERSION>::default(), data }
	}
}

// Add a helper method to get the version
impl<const VERSION: u32> BoundedVersion<VERSION> {
    pub fn version(&self) -> u32 { self.0 }
}

pub fn serialize_as_versioned_message<T>(data: T) -> Result<Vec<u8>, ProtocolError>
where
	T: serde::Serialize,
{
	// let network_namespace_id = network_namespace::get_network_namespace_id().to_string();
	// debug!("🏥 Network - Protocol | Serialize Versioned Message | Network namespace ID: {}", network_namespace_id);
	// let versioned = VersionedMessage::<PROTOCOL_VERSION, T>::new(data, &network_namespace_id);
	let versioned = VersionedMessage::<PROTOCOL_VERSION, T>::new(data);
	Ok(bincode::serialize(&versioned).map_err(|e| ProtocolError::Bincode(e))?)
}

pub fn deserialize_from_versioned_message<'a, 'b, T>(data: &'a Vec<u8>) -> Result<T, ProtocolError>
where
	T: serde::Deserialize<'b>,
	'a: 'b,
{
	// let network_namespace_id = network_namespace::get_network_namespace_id().to_string(); 
	let versioned = bincode::deserialize::<VersionedMessage<PROTOCOL_VERSION, T>>(data).map_err(|e| ProtocolError::Bincode(e))?;
	
	if versioned.protocol_version.version() != PROTOCOL_VERSION {
		return Err(ProtocolError::VersionMismatch { expected: PROTOCOL_VERSION, received: versioned.protocol_version.0 });
	}
	// if versioned.network_namespace_id != network_namespace_id {
		// return Err(ProtocolError::NamespaceMismatch { expected: network_namespace_id, received: versioned.network_namespace_id });
	// }
	
	Ok(versioned.data)
}
