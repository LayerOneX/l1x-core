use anyhow::{anyhow, Error};
use secp256k1::rand::rngs::OsRng;

use secp256k1::{All, PublicKey, Secp256k1, SecretKey};

#[derive(Debug, PartialEq)]
pub struct KeySpace {
	pub secret_key: SecretKey,
	pub public_key: PublicKey,
}

impl KeySpace {
	/// Creates a new KeySpace
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let key_space = KeySpace::new();
	/// ```
	pub fn new() -> Self {
		let secp_256k1 = Secp256k1::new();
		let (secret_key, public_key) = secp_256k1.generate_keypair(&mut OsRng);

		KeySpace { secret_key, public_key }
	}

	/// Creates a KeySpace from bytes
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let key_space = KeySpace::new();
	/// let bytes = key_space.to_bytes_key_space();
	/// let key_space_from_bytes = KeySpace::from_bytes_key_space(&bytes).unwrap();
	/// assert_eq!(key_space, key_space_from_bytes);
	/// ```
	pub fn from_bytes_key_space(bytes: &[u8]) -> Result<Self, Error> {
		let secret_key = match SecretKey::from_slice(&bytes[0..32]) {
			Ok(secret_key) => secret_key,
			Err(err) =>
				return {
					let message = format!("Invalid private key: {}", err);
					Err(anyhow!(message))
				},
		};
		let public_key = match PublicKey::from_slice(&bytes[32..]) {
			Ok(public_key) => public_key,
			Err(err) =>
				return {
					let message = format!("Invalid public key: {}", err);
					Err(anyhow!(message))
				},
		};

		Ok(KeySpace { secret_key, public_key })
	}

	/// Creates a Secp256k1 PublicKey from bytes
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let key_space = KeySpace::new();
	/// let bytes = key_space.to_bytes_public_key();
	/// let public_key_from_bytes = KeySpace::public_key_from_bytes(&bytes).unwrap();
	/// assert_eq!(key_space.public_key, public_key_from_bytes);
	/// ```
	/// # Errors
	/// Returns an error if the bytes are not a valid public key
	pub fn public_key_from_bytes(bytes: &[u8]) -> Result<PublicKey, Error> {
		let public_key = match PublicKey::from_slice(bytes) {
			Ok(public_key) => public_key,
			Err(err) => {
				let message = format!("Invalid public key: {}", err);
				return Err(anyhow!(message))
			},
		};
		Ok(public_key)
	}

	/// Creates a Secp256k1 SecretKey from bytes
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let key_space = KeySpace::new();
	/// let bytes = key_space.to_bytes_secret_key();
	/// let secret_key_from_bytes = KeySpace::secret_key_from_bytes(&bytes).unwrap();
	/// assert_eq!(key_space.secret_key, secret_key_from_bytes);
	/// ```
	pub fn secret_key_from_bytes(bytes: &[u8]) -> Result<SecretKey, Error> {
		let secret_key = match SecretKey::from_slice(bytes) {
			Ok(secret_key) => secret_key,
			Err(err) =>
				return {
					let message = format!("Invalid private key: {}", err);
					Err(anyhow!(message))
				},
		};
		Ok(secret_key)
	}

	/// Converts a KeySpace to bytes
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let key_space = KeySpace::new();
	/// let bytes = key_space.to_bytes_key_space();
	/// let key_space_from_bytes = KeySpace::from_bytes_key_space(&bytes).unwrap();
	/// assert_eq!(key_space, key_space_from_bytes);
	/// ```
	pub fn to_bytes_key_space(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend_from_slice(&self.secret_key[..]);
		bytes.extend_from_slice(&self.public_key.serialize()[..]);
		bytes
	}

	/// Converts a PublicKey to bytes
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let key_space = KeySpace::new();
	/// let bytes = key_space.to_bytes_public_key();
	/// let public_key_from_bytes = KeySpace::public_key_from_bytes(&bytes).unwrap();
	/// assert_eq!(key_space.public_key, public_key_from_bytes);
	/// ```
	pub fn to_bytes_public_key(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend_from_slice(&self.public_key.serialize()[..]);
		bytes
	}

	/// Converts a SecretKey to bytes
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let key_space = KeySpace::new();
	/// let bytes = key_space.to_bytes_secret_key();
	/// let secret_key_from_bytes = KeySpace::secret_key_from_bytes(&bytes).unwrap();
	/// assert_eq!(key_space.secret_key, secret_key_from_bytes);
	/// ```
	pub fn to_bytes_secret_key(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend_from_slice(&self.secret_key[..]);
		bytes
	}

	/// Creates a PublicKey from a SecretKey
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let secp = secp256k1::Secp256k1::new();
	/// let key_space = KeySpace::new();
	/// let public_key = KeySpace::public_key_from_secret_key(&secp, &key_space.secret_key);
	/// assert_eq!(key_space.public_key, public_key);
	/// ```
	pub fn public_key_from_secret_key(secp: &Secp256k1<All>, secret_key: &SecretKey) -> PublicKey {
		PublicKey::from_secret_key(secp, secret_key)
	}
}
