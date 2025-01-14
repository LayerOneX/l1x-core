use anyhow::{anyhow, Error};
use bincode::Result as BincodeResult;
use secp256k1::{
	ecdsa::Signature as EcdsaSignature, hashes::sha256, Message, PublicKey, SecretKey,
};
use serde::{Deserialize, Serialize};

/// Trait for converting a type to bytes
pub trait ByteOps {
	fn to_bytes(&self) -> BincodeResult<Vec<u8>>;
}

/// Trait for converting bytes to a type
impl<T: Serialize> ByteOps for T {
	fn to_bytes(&self) -> BincodeResult<Vec<u8>> {
		bincode::serialize(self)
	}
}

/// Trait crypto operations
pub trait SecpVRF {
	/// Get a message from a type
	/// Returns a Result
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// use serde::{Deserialize, Serialize};
	/// #[derive(Serialize, Deserialize, Debug)]
	/// struct Payload {
	///          pub message: String
	/// }
	/// let payload = Payload {
	///     message: "Hello World".to_string()
	/// };
	/// let message = payload.get_message().unwrap();
	/// ```
	fn get_message(&self) -> Result<Message, Error>;

	/// Verify a type with a public key and signature
	/// Returns a Result
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// use serde::{Deserialize, Serialize};
	/// #[derive(Serialize, Deserialize, Debug)]
	/// struct Payload {
	///          pub message: String
	/// }
	/// let payload = Payload {
	///    message: "Hello World".to_string()
	/// };
	/// let key_space = KeySpace::new();
	/// let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
	/// let verified = payload.verify_with_ecdsa(&key_space.public_key, signature);
	fn verify_with_ecdsa(
		&self,
		public_key: &PublicKey,
		signature: EcdsaSignature,
	) -> Result<(), Error>;

	/// Sign a type with a secret key
	/// Returns a Result
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// use serde::{Deserialize, Serialize};
	/// #[derive(Serialize, Deserialize, Debug)]
	/// struct Payload {
	///         pub message: String
	/// }
	/// let payload = Payload {
	///    message: "Hello World".to_string()
	/// };
	/// let key_space = KeySpace::new();
	/// let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
	/// ```
	fn sign_with_ecdsa(&self, secret_key: SecretKey) -> Result<EcdsaSignature, Error>;
}

// trait implementation for slices
impl SecpVRF for [u8] {
	/// Get a message from a slice
	/// Returns a Result
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// let message = "Hello World".as_bytes();
	/// let message = message.get_message().unwrap();
	/// ```
	/// # Errors
	/// Returns an error if the bytes are not a valid message
	fn get_message(&self) -> Result<Message, Error> {
		let message = Message::from_hashed_data::<sha256::Hash>(self);
		Ok(message)
	}

	/// Verify a slice with a public key and signature
	/// # Arguments
	/// `public_key` - Public Key
	/// `signature` - Signature
	/// Returns a Result
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let message = "Hello World".as_bytes();
	/// let key_space = KeySpace::new();
	/// let public_key = key_space.public_key;
	/// let signature = message.sign_with_ecdsa(key_space.secret_key).unwrap();
	/// let verified = message.verify_with_ecdsa(&public_key, signature);
	/// ```
	/// # Errors
	/// Returns an error if the signature is not valid
	fn verify_with_ecdsa(
		&self,
		public_key: &PublicKey,
		signature: EcdsaSignature,
	) -> Result<(), Error> {
		let message = Self::get_message(self)?;
		match signature.verify(&message, public_key) {
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow!("Signature is not valid: {}", e)),
		}
	}

	/// Sign a slice with a secret key
	/// # Arguments
	/// `secret_key` - Secret Key
	/// Returns a signature
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// let message = "Hello World".as_bytes();
	/// let key_space = KeySpace::new();
	/// let signature = message.sign_with_ecdsa(key_space.secret_key).unwrap();
	/// ```
	fn sign_with_ecdsa(&self, secret_key: SecretKey) -> Result<EcdsaSignature, Error> {
		let message = Self::get_message(self)?;
		let signature = secret_key.sign_ecdsa(message);
		Ok(signature)
	}
}

/// Trait Implementation for Structs
impl<T: Serialize + Deserialize<'static>> SecpVRF for T {
	/// Get a message from a struct
	/// Returns a Result
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use serde::{Deserialize, Serialize};
	/// use l1x_vrf::common::SecpVRF;
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// #[derive(Serialize, Deserialize, Debug)]
	/// struct Payload {
	///           pub message: String
	/// }
	/// let payload = Payload {
	///  message: "Hello World".to_string()
	/// };
	///
	/// let message = payload.get_message().unwrap();
	/// ```
	fn get_message(&self) -> Result<Message, Error> {
		let json_str_result = serde_json::to_string(&self);
		let json_str = match json_str_result {
			Ok(str) => str,
			Err(_err) => return Err(anyhow!("Error converting struct to json string")),
		};
		let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());
		Ok(message)
	}

	/// Verify a struct with a public key and signature
	/// `public_key` - Public Key
	/// `signature` - Signature
	/// Returns a Result
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// use serde::{Deserialize, Serialize};
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// #[derive(Serialize, Deserialize, Debug)]
	/// struct Payload {
	///            pub message: String
	/// }
	/// let payload = Payload {
	///   message: "Hello World".to_string()
	/// };
	/// let key_space = KeySpace::new();
	/// let public_key = key_space.public_key;
	/// let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
	/// let verified = payload.verify_with_ecdsa(&public_key, signature);
	/// ```
	fn verify_with_ecdsa(
		&self,
		public_key: &PublicKey,
		signature: EcdsaSignature,
	) -> Result<(), Error> {
		let json_str_result = serde_json::to_string(&self);
		let json_str = match json_str_result {
			Ok(str) => str,
			Err(err) => {
				let message = format!("Error converting struct to json string {:?}", err);
				return Err(anyhow!(message))
			},
		};
		let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());

		match signature.verify(&message, public_key) {
			Ok(_) => Ok(()),
			Err(e) => {
				let message = format!("Signature is not valid {:?}", e);
				// log::error!("{}", message);
				Err(anyhow!(message))
			},
		}
	}

	/// Sign a struct with a secret key
	/// `secret_key` - Secret Key
	/// Returns a signature
	/// # Example
	/// ```
	/// use secp256k1::{PublicKey, SecretKey};
	/// use l1x_vrf::common::SecpVRF;
	/// use l1x_vrf::secp_vrf::KeySpace;
	/// use serde::{Deserialize, Serialize};
	/// #[derive(Serialize, Deserialize, Debug)]
	/// struct Payload {
	///           pub message: String
	/// }
	/// let payload = Payload {
	///  message: "Hello World".to_string()
	/// };
	/// let key_space = KeySpace::new();
	/// let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
	/// ```
	fn sign_with_ecdsa(&self, secret_key: SecretKey) -> Result<EcdsaSignature, Error> {
		let json_str_result = serde_json::to_string(&self);
		let json_str = match json_str_result {
			Ok(str) => str,
			Err(_err) => return Err(anyhow!("Error converting struct to json string")),
		};

		let message = Message::from_hashed_data::<sha256::Hash>(json_str.as_bytes());
		let _message_hex = hex::encode(message.as_ref());
		let signature = secret_key.sign_ecdsa(message);
		Ok(signature)
	}
}

/// Get a signature from bytes
/// Returns a Result
/// # Example
/// ```
/// use secp256k1::{PublicKey, SecretKey};
/// use l1x_vrf::common::{get_signature_from_bytes, SecpVRF};
/// use l1x_vrf::secp_vrf::KeySpace;
/// use serde::{Deserialize, Serialize};
/// #[derive(Serialize, Deserialize, Debug)]
/// struct Payload {
///  pub message: String
/// }
/// let payload = Payload {
/// message: "Hello World".to_string()
/// };
/// let key_space = KeySpace::new();
/// let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
/// let signature_bytes = signature.serialize_compact();
/// let signature_from_bytes = get_signature_from_bytes(&signature_bytes).unwrap();
/// assert_eq!(signature, signature_from_bytes);
/// ```
pub fn get_signature_from_bytes(bytes: &[u8]) -> Result<EcdsaSignature, Error> {
	let signature = EcdsaSignature::from_compact(bytes)?;
	Ok(signature)
}
