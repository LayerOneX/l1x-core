use bip39::{Language, Mnemonic, Seed};
use block::block_state::BlockState;
use block_proposer::block_proposer_state::BlockProposerState;
use db::db::DbTxConn;
use hex;
use l1x_vrf::secp_vrf::KeySpace;
use primitives::{
	self, Address, Balance, BlockHash, BlockNumber, BlockTimeInSecond, BlockVersion, Epoch, MaximumFee, MinimumFee
};
use secp256k1::Secp256k1;
use serde::{Deserialize, Serialize};
use std::{fmt, time::SystemTime};

use system::{block_header::BlockHeader, block_proposer::BlockProposer, validator::Validator};

#[derive(Debug, Clone, PartialEq)]
pub enum ChainType {
	Mainnet,
	Testnet,
	Localnet,
}

impl ChainType {
	pub fn is_mainnet(&self) -> bool {
		matches!(self, Self::Mainnet)
	}
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenesisAccount {
	pub nickname: String,
	pub address: String,
	pub balance: Balance,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenesisValidator {
	pub address: Address,
	pub cluster_address: Address,
	pub epoch: Epoch,
	pub stake: Balance,
	// Old genesis.json doesn't support xscore
	pub xscore: Option<f64>
}

impl Into<Validator> for GenesisValidator {
	fn into(self) -> Validator {
		(&self).into()
	}
}

impl Into<Validator> for &GenesisValidator {
	fn into(self) -> Validator {
		Validator {
			address: self.address,
			epoch: self.epoch,
			cluster_address: self.cluster_address,
			stake: self.stake,
			xscore: self.xscore.unwrap_or(1.0)
		}
	}
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenesisData {
	pub genesis_time: SystemTime,
	pub genesis_amount: Balance,
	pub block_version: BlockVersion,
	pub block_number: BlockNumber,
	pub block_time_in_second: BlockTimeInSecond,
	pub maximum_fee: MaximumFee,
	pub minimum_fee: MinimumFee,
	pub accounts: Vec<GenesisAccount>,
	pub validators: Vec<GenesisValidator>,
	pub staking_pool_address: Address,
}

// impl GenesisData {
// 	fn new(dev_mode: bool) -> Self {
// 		let mut genesis = Genesis::new(Self {
// 			genesis_time: SystemTime::now(),
// 			genesis_amount: 200000000000000000000000000,
// 			block_version: 1,
// 			block_number: 0,
// 			block_time_in_second: 10,
// 			maximum_fee: 1_000_000,
// 			minimum_fee: 1_000,
// 			accounts: vec![],
// 			validators: vec![],
// 			staking_pool_address: [0u8; 20],
// 		}, dev_mode);

// 		//create multiple account
// 		let addresses = genesis.accounts(dev_mode);

// 		//generate the validators
// 		let genesis_validators = genesis.validators();

// 		genesis.data.accounts = addresses;
// 		genesis.data.validators = genesis_validators;
// 		genesis.data.staking_pool_address = hex::decode("522b3294fe78d57a1d7e1c37393f11841f6a9494")
// 			.expect("Unable to decode")
// 			.try_into()
// 			.expect("Wrong length of Vec");
// 		genesis.data
// 	}
// }

#[derive(Debug, Serialize, Deserialize)]
pub struct Genesis {
	pub data: GenesisData,
}

// impl Default for GenesisData {
// 	fn default() -> Self {
// 		let mut genesis = Genesis::new(Self {
// 			genesis_time: SystemTime::now(),
// 			genesis_amount: 200000000000000000000000000,
// 			block_version: 1,
// 			block_number: 0,
// 			block_time_in_second: 10,
// 			maximum_fee: 1_000_000,
// 			minimum_fee: 1_000,
// 			accounts: vec![],
// 			validators: vec![],
// 			staking_pool_address: [0u8; 20],
// 		});

// 		//create multiple account
// 		let addresses = genesis.accounts();

// 		//generate the validators
// 		let genesis_validators = genesis.validators();

// 		genesis.data.accounts = addresses;
// 		genesis.data.validators = genesis_validators;
// 		genesis.data.staking_pool_address = hex::decode("522b3294fe78d57a1d7e1c37393f11841f6a9494")
// 			.expect("Unable to decode")
// 			.try_into()
// 			.expect("Wrong length of Vec");
// 		genesis.data
// 	}
// }

// impl Default for Genesis {
// 	fn default() -> Self {
// 		Self { data: GenesisData::default() }
// 	}
// }
impl<'a> Genesis {
	pub fn new(dev_mode: bool) -> Self {
		let genesis_data = GenesisData {
			genesis_time: SystemTime::now(),
			genesis_amount: 200000000000000000000000000,
			block_version: 1,
			block_number: 0,
			block_time_in_second: 10,
			maximum_fee: 1_000_000,
			minimum_fee: 1_000,
			accounts: vec![],
			validators: vec![],
			staking_pool_address: [0u8; 20],
		};

		let mut genesis = Genesis { data: genesis_data };

		//create multiple account
		let addresses = genesis.accounts(dev_mode);

		//generate the validators
		let genesis_validators = genesis.validators();

		genesis.data.accounts = addresses;
		genesis.data.validators = genesis_validators;
		genesis.data.staking_pool_address = hex::decode("522b3294fe78d57a1d7e1c37393f11841f6a9494")
			.expect("Unable to decode")
			.try_into()
			.expect("Wrong length of Vec");
		genesis
	}
	pub fn genesis_time(&self) -> SystemTime {
		self.data.genesis_time
	}

	pub async fn set_block_proposer(
		&self,
		cluster_address: Address,
		epoch: Epoch,
		_address: Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> BlockProposer {
		let address: Address = hex::decode("78e044394595d4984f66c1b19059bc14ecc24063")
			.expect("Unable to decode")
			.try_into()
			.expect("Wrong length of Vec");
		let block_proposer: BlockProposer =
			BlockProposer { cluster_address, epoch, address };

		let block_proposer_state = BlockProposerState::new(db_pool_conn)
			.await
			.expect("Error creating BlockProposerState");
		block_proposer_state
			.raw_query("TRUNCATE block_proposer")
			.await
			.expect("Unable to TRUNCATE block_proposer");

		block_proposer_state
			.store_block_proposer(cluster_address, epoch, address)
			.await
			.expect("Failed to store block proposer");
		block_proposer
	}

	pub async fn set_genesis_block_header(
		&self,
		cluster_address: Address,
		db_pool_conn: &'a DbTxConn<'a>,
	) -> anyhow::Result<()> {
		let genesis = &self.data;
		let block_state = BlockState::new(&db_pool_conn).await?;
		let block_header= BlockHeader {
			block_hash: BlockHash::default(),
			block_number: genesis.block_number,
			parent_hash: BlockHash::default(),
			block_type: system::block::BlockType::L1XTokenBlock,
			cluster_address,
			// Same timestamp should be used on all nodes. `genesis_time` is incorect because it uses the current time.
			// So 0 is used here
			timestamp: 0, 
			num_transactions: 0,
			block_version: 0,
			state_hash: BlockHash::default(),
			epoch: 1
		};
		block_state.store_block_header(block_header.clone()).await?;
		block_state.update_block_head(block_header.cluster_address, block_header.block_number, block_header.block_hash).await?;
		Ok(())
	}

	/// Form the array of genesis accounts that the chain will start with. Initial L1X token
	/// creation will start here.
	pub fn accounts(&self, dev_mode: bool) -> Vec<GenesisAccount> {
		let mut account_addresses: Vec<(String, String)> = Vec::new();
		let mut genesis_accounts: Vec<GenesisAccount> = Vec::new();

		let l1x_validator_1_account: String =
			"78e044394595d4984f66c1b19059bc14ecc24063".to_string();
		let l1x_validator_2_account: String =
			"7b7ab20f75b691e90c546e89e41aa23b0a821444".to_string();

		let l1x_dev_1_account: String = "75104938baa47c54a86004ef998cc76c2e616289".to_string();
		let l1x_dev_2_account: String = "50028cf7ed245e4ac9e472d5277f14ed1c7ab384".to_string();
		let l1x_dev_3_account: String = "4489da9d81f0bc8125c8efdda1c117a7a895b43d".to_string();
		let l1x_dev_4_account: String = "3b647b46c9ba4fca221ccf933c09b653c2b4581f".to_string();

		account_addresses.push(("Genesis Validator 1".to_string(), l1x_validator_1_account));
		account_addresses.push(("Genesis Validator 2".to_string(), l1x_validator_2_account));

		if dev_mode {
			account_addresses.push(("L1X Dev 1 Account".to_string(), l1x_dev_1_account));
			account_addresses.push(("L1X Dev 2 Account".to_string(), l1x_dev_2_account));
			account_addresses.push(("L1X Dev 3 Account".to_string(), l1x_dev_3_account));
			account_addresses.push(("L1X Dev 4 Account".to_string(), l1x_dev_4_account));
		}

		// Genesis account
		let genesis_account = GenesisAccount {
			nickname: "Genesis Account".to_string(),
			address: "81ca4e520742040836bbcfa4647b4463be13768b".to_string(),
			balance: 100000000000000000000000000, // balance of the accounts.
		};
		genesis_accounts.push(genesis_account);

		// Initialize the Secp256k1 context
		// let secp = Secp256k1::new();
		// for _ in 1..5 {
		// // Generate a new mnemonic
		// let mnemonic = Mnemonic::new(MnemonicType::Words24, Language::English);

		// // Create a Seed from the mnemonic
		// let seed = Seed::new(&mnemonic, "");

		// // Generate a secp256k1 SecretKey from the first 32 bytes of the seed
		// let sk = KeySpace::secret_key_from_bytes(&(seed.as_bytes()[0..32])).unwrap();

		// // Generate the corresponding public key
		// let pk = KeySpace::public_key_from_secret_key(&secp, &sk);
		// let serialized = pk.serialize();

		// // Convert to Address
		// let mut address: Address = [0; 20];
		// address.copy_from_slice(&serialized[1..21]); // Take a 20-byte slice

		// let address_hex = hex::encode(address);
		// account_addresses.push(address_hex);
		// }

		for (nickname, address_hex) in account_addresses.iter() {
			let account = GenesisAccount {
				nickname: nickname.clone(),
				address: address_hex.clone(),
				balance: 100000000000000000000000000000000000000, // balance of the accounts.
			};
			genesis_accounts.push(account);
		}

		genesis_accounts
	}

	pub fn recover_account(mnemonic_str: &str) -> Result<Address, String> {
		// Create a Mnemonic from the provided mnemonic string
		let mnemonic =
			Mnemonic::from_phrase(mnemonic_str, Language::English).map_err(|e| e.to_string())?;

		// Create a Seed from the mnemonic
		let seed = Seed::new(&mnemonic, "");

		// Initialize the Secp256k1 context
		let secp = Secp256k1::new();

		// Generate a secp256k1 SecretKey from the first 32 bytes of the seed
		let sk = KeySpace::secret_key_from_bytes(&(seed.as_bytes()[0..32])).unwrap();

		// Generate the corresponding public key
		let pk = KeySpace::public_key_from_secret_key(&secp, &sk);
		let serialized = pk.serialize();

		// Convert to Address
		let mut address: Address = [0; 20];
		address.copy_from_slice(&serialized[1..21]); // Take a 20-byte slice
		let address_hex = hex::encode(address);
		println!("address for account {}", address_hex);

		Ok(address)
	}

	pub fn validators(&self) -> Vec<GenesisValidator> {
		let genesis_validator1 = GenesisValidator {
			// address: "78e044394595d4984f66c1b19059bc14ecc24063".to_string(),
			address: hex::decode("78e044394595d4984f66c1b19059bc14ecc24063")
				.expect("Unable to decode")
				.try_into()
				.expect("Wrong length of Vec"),
			epoch: 0,
			cluster_address: [1u8; 20],
			stake: 100_000u128,
			xscore: Some(1.0)
		};

		let genesis_validator2 = GenesisValidator {
			address: hex::decode("7b7ab20f75b691e90c546e89e41aa23b0a821444")
				.expect("Unable to decode")
				.try_into()
				.expect("Wrong length of Vec"),
			epoch: 0,
			cluster_address: [1u8; 20],
			stake: 100_000u128,
			xscore: Some(1.0)
		};

		// Create a Vec and populate it
		let validators = vec![genesis_validator1, genesis_validator2];

		validators
	}
}
impl fmt::Display for GenesisAccount {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"Nickname: {}, Address: 0x{}, Balance: {}",
			self.nickname, self.address, self.balance
		)
	}
}
