#[cfg(test)]
mod tests {
	use crate::{
		account::Account, node_health::NodeHealth, node_info::*, transaction::{TXV1SignPayload, Transaction, TransactionType}
	};
	use eth_keystore::decrypt_key;
	use ethereum_types::{H160, H520};
	use l1x_vrf::{
		common::{get_signature_from_bytes, SecpVRF},
		secp_vrf::KeySpace,
	};
	use primitives::*;
	use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1, SecretKey};
	use serde::{Deserialize, Serialize};
	use sha3::{Digest, Keccak256};
	use std::{path::PathBuf, str::FromStr};
	use types::eth::bytes::Bytes;

	#[derive(Serialize, Deserialize)]
	struct SigningPayload {
		nonce: Nonce,
		fee_limit: Balance,
		transaction_type: TransactionType,
	}

	#[derive(Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, Debug, Default)]
	struct SignNodeInfoPayload {
		pub address: Address,
		#[serde(with = "serde_bytes")]
		pub ip_address: IpAddress,
		#[serde(with = "serde_bytes")]
		pub metadata: Metadata,
	}

	#[test]
	fn test_transaction_verify_signature_valid() {
		// Create a valid transaction
		let nonce = 1;
		let recipient: Address = [2u8; 20].into();
		let amount = 100;
		let fee_limit = 10;

		let key_space = KeySpace::new();

		let payload = TXV1SignPayload {
			nonce,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
		};

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let signature_bytes = signature.serialize_compact().to_vec();

		let verifying_key = key_space.public_key;

		let constructed_signature = get_signature_from_bytes(&signature_bytes).unwrap();
		assert_eq!(signature, constructed_signature);

		let transaction = Transaction::new(
			nonce,
			TransactionType::NativeTokenTransfer(recipient, amount),
			fee_limit,
			signature,
			verifying_key,
		);

		let result = transaction.verify_signature();

		assert_eq!(result.unwrap(), true);
	}

	#[test]
	fn test_transaction_verify_signature_invalid() {
		let nonce = 1;
		let recipient: Address = [2u8; 20].into();
		let amount = 100;
		let fee_limit = 10;

		let key_space = KeySpace::new();

		let payload = SigningPayload {
			nonce,
			fee_limit,
			transaction_type: TransactionType::NativeTokenTransfer(recipient, amount),
		};

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let verifying_key = key_space.public_key;

		let transaction = Transaction::new(
			nonce,
			// Modify the recipient address to make the signature invalid
			TransactionType::NativeTokenTransfer([3u8; 20], amount),
			fee_limit,
			signature,
			verifying_key,
		);

		// Verify the signature
		let result = transaction.verify_signature();

		// Assert that the signature verification fails
		assert_eq!(result.unwrap(), false);
	}

	#[tokio::test]
	async fn test_node_info_verify_signature_valid() {
		// Create a valid transaction
		let key_space = KeySpace::new();

		let mut node_health_new = Vec::new();

		let node_health = 	NodeHealth{
			measured_peer_id: "measured peer".to_string(),
			peer_id: "peer_id".to_string(),
			epoch: 1,
			joined_epoch: 1,
			uptime_percentage: 0.0,
			response_time_ms: 0,
			transaction_count: 0,
			block_proposal_count: 0,
			anomaly_score: 0.0,
			node_health_version: 1,
		};

		node_health_new.push(node_health);

		let payload = NodeInfoSignPayload {
			ip_address: IpAddress::from("127.0.0.1".to_string()),
			metadata: "metadata".to_string().into_bytes(),
			cluster_address: [1u8; 20],
		};

		let signature: Signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();

		// creating a byte array out of signature
		let serialized_signature_byte = signature.serialize_compact().to_vec();
		let reconstructed_signature =
			get_signature_from_bytes(&serialized_signature_byte.clone()).unwrap();

		assert_eq!(signature, reconstructed_signature);

		let public_key = key_space.public_key;
		let ip_address = "127.0.0.1".as_bytes().to_vec();
		let metadata = "metadata".as_bytes().to_vec();
		let cluster_address = [1u8; 20];
		let address = Account::address(&public_key.serialize().to_vec().clone()).unwrap();
		let node_info = NodeInfo::new(
			address,
			"node_id".as_bytes().to_vec(),
			node_health_new,
			ip_address,
			metadata,
			cluster_address,
			public_key.serialize().to_vec(),
			signature.serialize_compact().to_vec(),
		);

		// Verify the signature
		let result = node_info.verify_signature().await;

		// Assert that the signature verification is successful
		assert_eq!(result.is_ok(), true);
	}

	#[tokio::test]
	async fn test_node_info_verify_signature_invalid() {
		// Create an invalid transaction
		let ip_address = "127.0.0.1".to_string();
		let metadata = "metadata".to_string();

		let key_space = KeySpace::new();
		let payload = NodeInfoSignPayload {
			ip_address: IpAddress::from(ip_address.clone()),
			metadata: metadata.clone().into_bytes(),
			cluster_address: [1u8; 20],
		};

		let signature = payload.sign_with_ecdsa(key_space.secret_key).unwrap();
		let public_key = key_space.public_key;
		let ip_address = "127.0.0.1".as_bytes().to_vec();
		let metadata = "metadata".as_bytes().to_vec();
		let cluster_address = [1u8; 20];
		let address = Account::address(&public_key.serialize().to_vec().clone()).unwrap();

		let mut node_health_new = Vec::new();

		let node_health = 	NodeHealth{
			measured_peer_id: "measured peer".to_string(),
			peer_id: "peer_id".to_string(),
			epoch: 1,
			joined_epoch: 1,
			uptime_percentage: 0.0,
			response_time_ms: 0,
			transaction_count: 0,
			block_proposal_count: 0,
			anomaly_score: 0.0,
			node_health_version: 1,
		};

		node_health_new.push(node_health);

		let mut node_info = NodeInfo::new(
			address,
			"node_id".as_bytes().to_vec(),
			node_health_new,
			ip_address,
			metadata,
			cluster_address,
			public_key.serialize().to_vec(),
			signature.serialize_compact().to_vec(),
		);

		// Modify the recipient address to make the signature invalid
		node_info.data.cluster_address = [2u8; 20].into();
		let result = node_info.verify_signature().await;
		// Assert that the signature verification fails
		assert_eq!(result.is_ok(), false);
	}

	#[test]
	fn test_address() {
		let verifying_key_bytes: VerifyingKeyBytes = vec![
			2, 21, 237, 183, 233, 166, 79, 153, 112, 198, 13, 148, 184, 102, 183, 54, 134, 152, 13,
			115, 72, 116, 56, 42, 209, 0, 39, 0, 229, 216, 112, 217, 69,
		];
		let expected_address: Address = [
			117, 16, 73, 56, 186, 164, 124, 84, 168, 96, 4, 239, 153, 140, 199, 108, 46, 97, 98,
			137,
		];

		let result = Account::address(&verifying_key_bytes).unwrap();
		println!("{:?}", result);
		assert_eq!(result, expected_address);
	}

	#[test]
	fn test_generate_account() {
		let account = Account::generate_account("l1xtestpassword").unwrap();
		println!("{:?}", account.private_key);
		// Check private key format
		assert_eq!(account.private_key.len(), 66); // 0x + 64 hex chars

		// Check public key format
		assert_eq!(account.public_key.len(), 68); // 0x + 66 hex chars for compressed key

		// Verify the address is derived from the public key
		let public_key_bytes =
			hex::decode(&account.public_key[2..]).expect("Invalid hex in public key");
		let public_key = PublicKey::from_slice(&public_key_bytes).expect("Invalid public key");

		let hash = Keccak256::digest(&public_key.serialize_uncompressed()[1..]);
		let derived_address = H160::from_slice(&hash[12..]);
		assert_eq!(account.address, derived_address, "Derived address does not match");
	}

	#[test]
	fn test_import_account() {
		let private_key_hex = "358695f6845b387c8bae593f7c03432c15c695dbd46f40b5128d46757ca21d3d";
		let password = "l1xtestpassword";

		let account = Account::import_account(private_key_hex, password).unwrap();

		// Verify the private key
		assert_eq!(account.private_key, format!("0x{}", private_key_hex));

		// Verify the public key and address are derived correctly
		let secp = Secp256k1::new();
		let private_key_bytes = hex::decode(&private_key_hex).unwrap();
		let private_key = SecretKey::from_slice(&private_key_bytes).unwrap();
		let public_key = PublicKey::from_secret_key(&secp, &private_key);
		let public_key_serialized = public_key.serialize_uncompressed();
		let hash = Keccak256::digest(&public_key_serialized[1..]);
		let expected_address = H160::from_slice(&hash[12..]);

		assert_eq!(account.address, expected_address);

		// Check if the keystore file is created
		let keystore_dir = PathBuf::from("../keystore");
		let keystore_path = keystore_dir.join(format!("{:?}", account.address));

		assert!(keystore_path.exists(), "Keystore file not found");

		// Load and decrypt the keystore file
		let decrypted_private_key = decrypt_key(&keystore_path, password).unwrap();
		assert_eq!(hex::encode(decrypted_private_key), private_key_hex);
	}

	#[test]
	fn unlock_account() {
		let private_key_hex = "358695f6845b387c8bae593f7c03432c15c695dbd46f40b5128d46757ca21d3d";
		let password = "l1xtestpassword";
		// Check if the keystore file is created
		let keystore_dir = PathBuf::from("../keystore");

		let account = Account::import_account(private_key_hex, password).unwrap();
		let keystore_path = keystore_dir.join(format!("{:?}", account.address));
		assert!(keystore_path.exists(), "Keystore file not found");
		// Verify the private key
		assert_eq!(account.private_key, format!("0x{}", private_key_hex));
		// View private_key from address
		let retrieved_private_key =
			Account::unlock_account(&format!("{:?}", account.address), password, None).unwrap();

		assert_eq!(hex::encode(retrieved_private_key), private_key_hex, "Keys don't match");
	}

	#[test]
	fn list_accounts() {
		let accounts = Account::list_accounts();
		assert!(accounts.is_ok())
	}

	#[test]
	fn test_sign() {
		let message_bytes = "test message".as_bytes();
		// Hash the message to ensure it is 32 bytes
		let message_hash = Keccak256::digest(message_bytes);

		// Check the length of the message hash
		if message_hash.len() != 32 {
			return eprint!("Message hash must be 32 bytes");
		}
		let message = Bytes::new(message_hash.to_vec());
		let address = H160::from_str("0xb1ca56b5bb351540635b3ae5ecb463c97a126245").unwrap();
		let password = String::from("l1xtestpassword");

		// Call the sign function
		let signature_result = Account::sign(1, message, address, password).unwrap();

		// Convert H520 to a byte array for verification
		let signature_bytes: [u8; 65] = signature_result.0.into();
		let signature =
			Signature::from_compact(&signature_bytes[0..64]).expect("Invalid compact signature");

		// Derive the public key from the known private key
		let private_key = SecretKey::from_slice(&hex_literal::hex!(
			"358695f6845b387c8bae593f7c03432c15c695dbd46f40b5128d46757ca21d3d"
		))
		.expect("Invalid private key");
		let public_key = PublicKey::from_secret_key(&Secp256k1::new(), &private_key);

		// Recreate the message for verification
		let message =
			Message::from_slice(&Keccak256::digest(&message_bytes)).expect("Invalid message");

		// Verify the signature
		let is_valid = Secp256k1::new().verify_ecdsa(&message, &signature, &public_key).is_ok();
		assert!(is_valid, "Signature should be valid");
	}

	#[test]
	fn test_ec_recover() {
		let secp = Secp256k1::new();

		// Generate a private key (for testing)
		let private_key = SecretKey::new(&mut rand::thread_rng());
		let public_key = PublicKey::from_secret_key(&secp, &private_key);
		let public_key_serialized = public_key.serialize_uncompressed();

		// Calculate the expected Ethereum address
		let expected_address_hash = Keccak256::digest(&public_key_serialized[1..]);
		let expected_address = H160::from_slice(&expected_address_hash[12..]);

		// Create a test message and sign it
		let message = "Test message";
		let message_hash = Keccak256::digest(message.as_bytes());
		let message = Message::from_slice(&message_hash).unwrap();
		let signature = secp.sign_ecdsa_recoverable(&message, &private_key);
		let (recovery_id, sig_bytes) = signature.serialize_compact();
		let mut signature_bytes = [0u8; 65];
		signature_bytes[0..32].copy_from_slice(&sig_bytes[0..32]);
		signature_bytes[32..64].copy_from_slice(&sig_bytes[32..64]);
		signature_bytes[64] = recovery_id.to_i32() as u8 + 27; // Adjusting v

		// Call the ec_recover function
		let recovered_address =
			Account::ec_recover(Bytes::from(message_hash.to_vec()), H520::from(signature_bytes))
				.unwrap();

		// Compare the recovered address with the expected address
		assert_eq!(recovered_address, expected_address);
	}
}
