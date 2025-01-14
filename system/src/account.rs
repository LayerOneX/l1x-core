use anyhow::{anyhow, Error as AError};
use eth_keystore::{decrypt_key, encrypt_key};
use ethereum_types::{H160, H520, U128};
use ethers::utils::keccak256;
use k256::{elliptic_curve::sec1::ToEncodedPoint, PublicKey as K256PublicKey};
use primitives::*;
use secp256k1::{
	ecdsa::{RecoverableSignature, RecoveryId},
	rand::{self, RngCore},
	Message, PublicKey, Secp256k1, SecretKey,
};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::{fmt::Write, path::PathBuf, str::FromStr};
use types::eth::bytes::Bytes;

const KEYSTORE_DIR: &str = "../keystore";

#[derive(Debug)]
pub struct NewAccount {
	pub private_key: String,
	pub public_key: String,
	pub address: H160,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
	pub address: Address,
	pub balance: Balance,
	pub nonce: Nonce,
	pub account_type: AccountType,
}

unsafe impl Send for Account {}
unsafe impl Sync for Account {}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AccountType {
	System,
	User,
}

impl AccountType {
	pub fn as_str(&self) -> &'static str {
		match *self {
			AccountType::System => "System",
			AccountType::User => "User",
		}
	}
}

// impl FromSql<Accounttype, Pg> for AccountType {
// 	fn from_sql(bytes: PgValue) -> deserialize::Result<Self> {
// 		match bytes.as_bytes() {
// 			b"System" => Ok(AccountType::System),
// 			b"User" => Ok(AccountType::User),
// 			_ => Err("Unrecognized enum variant".into()),
// 		}
// 	}
// }

impl Account {
	pub fn new(address: Address) -> Account {
		Account { address, balance: 0, nonce: 0, account_type: AccountType::User }
	}

	pub fn new_system(address: Address) -> Account {
		Account { address, balance: 0, nonce: 0, account_type: AccountType::System }
	}

	pub fn address(verifying_key_bytes: &VerifyingKeyBytes) -> Result<Address, AError> {
		let public_key = match secp256k1::PublicKey::from_slice(verifying_key_bytes.as_slice()) {
			Ok(public_key) => public_key,
			Err(err) => {
				if *verifying_key_bytes == compile_time_config::SYSTEM_CONTRACTS_OWNER {
					return Ok(compile_time_config::SYSTEM_CONTRACTS_OWNER)
				} else if *verifying_key_bytes == compile_time_config::SYSTEM_REWARDS_DISTRIBUTOR {
					return Ok(compile_time_config::SYSTEM_REWARDS_DISTRIBUTOR)
				}
				return Err(anyhow!("Unable to construct public key {:?}", err))
			},
		};

		let k_pub_bytes =
			K256PublicKey::from_sec1_bytes(&public_key.serialize_uncompressed()).unwrap();

		let k_pub_bytes = k_pub_bytes.to_encoded_point(false);
		let k_pub_bytes = k_pub_bytes.as_bytes();

		let hash = keccak256(&k_pub_bytes[1..]);
		let mut bytes = [0u8; 20];
		bytes.copy_from_slice(&hash[12..]);
		Ok(bytes)
	}

	pub fn contract_address(
		account_address: &Address,
		cluster_address: &Address,
		nonce: Nonce,
	) -> Address {
		let mut input: Vec<u8> = Vec::new();
		//input.extend_from_slice(contract_code);
		input.extend_from_slice(account_address);
		input.extend_from_slice(cluster_address);
		input.extend_from_slice(&nonce.to_be_bytes());

		let hash = Keccak256::digest(&input);
		let mut address = [0u8; 20];
		address.copy_from_slice(&hash[12..]);
		address
	}

	pub fn contract_instance_address(
		account_address: &Address,
		contract_address: &Address,
		cluster_address: &Address,
		nonce: Nonce,
	) -> Address {
		let mut input: Vec<u8> = Vec::new();
		input.extend_from_slice(account_address);
		input.extend_from_slice(contract_address);
		input.extend_from_slice(cluster_address);
		input.extend_from_slice(&nonce.to_be_bytes());

		let hash = Keccak256::digest(&input);
		let mut address = [0u8; 20];
		address.copy_from_slice(&hash[12..]);
		address
	}

	pub fn pool_address(
		account_address: &Address,
		cluster_address: &Address,
		nonce: Nonce,
	) -> Address {
		let mut input: Vec<u8> = Vec::new();
		input.extend_from_slice(account_address);
		input.extend_from_slice(cluster_address);
		input.extend_from_slice(&nonce.to_be_bytes());

		let hash = Keccak256::digest(&input);
		let mut address = [0u8; 20];
		address.copy_from_slice(&hash[12..]);
		address
	}

	pub fn generate_account(password: &str) -> Result<NewAccount, AError> {
		let mut rng = rand::thread_rng();
		let mut priv_str = String::new();
		let mut pub_str = String::new();
		let secp = Secp256k1::new();
		// Create a new random private key
		let (private_key, public_key) = secp.generate_keypair(&mut rng);

		// Serialize the public key and take the Keccak-256 hash of the last 64 bytes
		let hash = Keccak256::digest(&public_key.serialize_uncompressed()[1..]);
		for byte in private_key.secret_bytes() {
			write!(priv_str, "{:02x}", byte).expect("Unable to write");
		}
		// Iterate over the bytes starting from the second byte, skipping the first one
		for byte in public_key.serialize() {
			write!(pub_str, "{:02x}", byte).expect("Unable to write");
		}
		let private_key = format!("0x{}", priv_str);
		let mut private_key_bytes = hex::decode(&private_key[2..])?;
		rng.fill_bytes(private_key_bytes.as_mut_slice());

		// Simplify the keystore file path
		let keystore_dir = PathBuf::from(KEYSTORE_DIR);
		if !keystore_dir.exists() {
			std::fs::create_dir_all(&keystore_dir)?; // Create the directory if it doesn't exist
		}

		let pub_key = format!("0x{}", pub_str);

		// Take the last 20 bytes of the hash to form the address
		let address = H160::from_slice(&hash[12..]);
		encrypt_key(
			&keystore_dir,
			&mut rng,
			&private_key_bytes,
			password,
			Some(&format!("{:?}", address)),
		)?;
		Self::new(address.0);
		Ok(NewAccount { private_key, public_key: pub_key, address })
	}

	pub fn import_account(private_key_hex: &str, password: &str) -> Result<NewAccount, AError> {
		let mut rng = rand::thread_rng();
		let secp = Secp256k1::new();
		let mut priv_str = String::new();
		let mut pub_str = String::new();
		// Decode the hex private key
		let private_key_bytes = hex::decode(private_key_hex)?;
		let private_key = SecretKey::from_slice(&private_key_bytes)?;

		// Derive the public key from the private key
		let public_key = PublicKey::from_secret_key(&secp, &private_key);
		let public_key_serialized = public_key.serialize_uncompressed();

		// Calculate the address from the public key
		let hash = Keccak256::digest(&public_key_serialized[1..]);
		let address = H160::from_slice(&hash[12..]);
		for byte in private_key.secret_bytes() {
			write!(priv_str, "{:02x}", byte).expect("Unable to write");
		}
		// Iterate over the bytes starting from the second byte, skipping the first one
		for byte in public_key.serialize() {
			write!(pub_str, "{:02x}", byte).expect("Unable to write");
		}
		let private_key = format!("0x{}", priv_str);
		let private_key_bytes = hex::decode(private_key_hex)?;
		let pub_key = format!("0x{}", pub_str);
		// Simplify the keystore file path
		let keystore_dir = PathBuf::from(KEYSTORE_DIR);
		if !keystore_dir.exists() {
			return Err(AError::msg("Keystore directory doesn't exist")); // return error if it doesn't exist
		}
		encrypt_key(
			&keystore_dir,
			&mut rng,
			&private_key_bytes,
			password,
			Some(&format!("{:?}", address)),
		)?;
		Self::new(address.0);
		Ok(NewAccount { private_key, public_key: pub_key, address })
	}

	pub fn unlock_account(
		address: &str,
		password: &str,
		_duration: Option<U128>,
	) -> Result<Vec<u8>, AError> {
		let keystore_dir = PathBuf::from(KEYSTORE_DIR);
		if !keystore_dir.exists() {
			return Err(AError::msg("Keystore directory doesn't exist")); // return error if it doesn't exist
		}
		let keystore_path = keystore_dir.join(address);
		// Decrypt the keystore file to get the private key
		let decrypted_key = decrypt_key(keystore_path, password)?;

		// Convert the decrypted private key to a hex string
		Ok(decrypted_key)
	}

	pub fn list_accounts() -> Result<Vec<H160>, AError> {
		let path = std::path::PathBuf::from(KEYSTORE_DIR);
		let mut accounts = Vec::new();

		// Iterate over the entries in the keystore directory
		match std::fs::read_dir(path) {
			Ok(entries) => {
				for entry in entries {
					match entry {
						Ok(entry) => {
							let path = entry.path();
							// Check if it is a file and not a directory
							if path.is_file() {
								// Get the filename as a string
								if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
									// Convert filename to H160 and add to accounts vector
									if let Ok(address) = H160::from_str(name) {
										accounts.push(address);
									}
								}
							}
						},
						Err(e) => return Err(e.into()), // Handle error reading the directory entry
					}
				}
			},
			Err(e) => return Err(e.into()), // Handle error reading the directory
		}

		Ok(accounts)
	}

	pub fn sign(
		chain_id: u64,
		message: Bytes,
		address: H160,
		password: String,
	) -> Result<H520, AError> {
		let secp = Secp256k1::new();

		// Replace with your method to unlock the account and get the private key bytes
		let private_key_bytes = Self::unlock_account(&format!("{:?}", address), &password, None)?;

		let secret = SecretKey::from_slice(&private_key_bytes)?;

		let signing_message = Message::from_slice(&message.0)?;

		let recoverable_sig = secp.sign_ecdsa_recoverable(&signing_message, &secret);
		let (rec_id, sig) = recoverable_sig.serialize_compact();

		// Compute v
		let v = {
			let recovery_id = rec_id.to_i32();
			let standard_v = if recovery_id % 2 == 0 { 27 } else { 28 };
			standard_v + 2 * chain_id + 8
		};

		// Concatenate r, s, and v to form the signature
		let mut signature_bytes = [0u8; 65];
		signature_bytes[0..32].copy_from_slice(&sig[0..32]); // Copy r
		signature_bytes[32..64].copy_from_slice(&sig[32..64]); // Copy s
		signature_bytes[64] = v as u8;

		Ok(H520::from(signature_bytes))
	}

	pub fn ec_recover(data: Bytes, signature: H520) -> Result<H160, AError> {
		let secp = Secp256k1::new();

		// Split the signature into r, s and v components
		let r = &signature[0..32];
		let s = &signature[32..64];
		let v = signature[64];

		// Create a recovery id
		let recovery_id = RecoveryId::from_i32(v as i32 - 27)?;

		// Create a message object from the message hash
		let message = Message::from_slice(data.0.as_slice())?;

		// Create the signature object from r and s components
		let sig = RecoverableSignature::from_compact(&[r, s].concat(), recovery_id)?;

		// Recover the public key
		let public_key = secp.recover_ecdsa(&message, &sig)?;
		let public_key_serialized = public_key.serialize_uncompressed();

		// Calculate the address from the public key
		let hash = Keccak256::digest(&public_key_serialized[1..]);

		Ok(H160::from_slice(&hash[12..]))
	}

	// fn sign_transaction(
	//     &self,
	//     request: TransactionRequest,
	//     password: String,
	// ) -> Result<RichRawTransaction, AError> {
	//     // Step 1: Unlock the account and get the private key
	//     let private_key_bytes = Self::unlock_account(&request.from, &password, None)?;
	//     let private_key = SecretKey::from_slice(&private_key_bytes)?;

	//     // Step 2: Construct the transaction
	//     // This includes serializing the transaction fields and RLP encoding
	//     let transaction = self.construct_transaction(&request)?;

	//     // Step 3: Sign the transaction
	//     let secp = Secp256k1::new();
	//     let message = Message::from_slice(&transaction.hash())?;
	//     let signature = secp.sign_ecdsa_recoverable(&message, &private_key);
	//     let serialized_signature = signature.serialize_compact();

	//     // Step 4: Append the signature to the transaction
	//     let signed_transaction = self.append_signature(transaction, serialized_signature)?;

	//     // Step 5: Generate the transaction hash
	//     let transaction_hash = self.calculate_transaction_hash(&signed_transaction);

	//     // Step 6: Return the signed transaction
	//     Ok(RichRawTransaction {
	//         raw: signed_transaction,
	//         transaction,
	//     })
	// }
}
