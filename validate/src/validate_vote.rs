use std::marker::PhantomData;
use anyhow::{anyhow, Error};
use db::db::DbTxConn;
use system::account::Account;
use system::block::Block;
use system::vote::Vote;
use validator::validator_state::ValidatorState;


pub struct ValidateVote<'a>{
	_marker: PhantomData<&'a ()>,
}

impl<'a> ValidateVote<'a> {
	pub async fn validate_vote(vote: &Vote, block: &Block, db_pool_conn: &'a DbTxConn<'a>,) -> Result<(), Error> {
		let validator_state = ValidatorState::new(&db_pool_conn).await?;
		let node_address =
			Account::address(&vote.verifying_key)?;
		if validator_state
			.is_validator(&node_address, vote.data.epoch)
			.await?
		{
			validate_vote_fields(vote, block).await?;
		}
		Ok(())
	}
}

async fn validate_vote_fields(vote: &Vote, block: &Block) -> Result<(), Error> {
	let block_number = block.block_header.block_number;
	if block.block_header.block_number != vote.data.block_number {
		return Err(anyhow!(
			"Vote: Incorrect block number: actual {}, expected: {}",
			vote.data.block_number,
			block.block_header.block_number
		));
	}
	if block.block_header.block_hash != vote.data.block_hash {
		return Err(anyhow!(
			"Vote: Incorrect block hash: actual {}, expected: {}, block #{}",
			hex::encode(&vote.data.block_hash),
			hex::encode(&block.block_header.block_hash),
			block_number,
		));
	}
	if block.block_header.cluster_address != vote.data.cluster_address {
		return Err(anyhow!(
			"Vote: Incorrect cluster address: actual {}, expected: {}, block #{}",
			hex::encode(&vote.data.cluster_address),
			hex::encode(&block.block_header.cluster_address),
			block_number,
		));
	}
	vote.verify_signature().await?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use l1x_vrf::common::SecpVRF;
	use system::vote::VoteSignPayload;
	use super::*;
	use secp256k1::{Secp256k1, SecretKey};
	use system::block::BlockType;
	use system::block_header::BlockHeader;

	#[tokio::test]
	async fn test_validate_validate_vote_fields() {
		let node_private_key = "f6b82b53ecbe1978b8651f740739b1d181f0285381e65e5e3491d8e821ab9bd0";
		let secret_key = SecretKey::from_slice(
			&hex::decode(node_private_key).expect("Error decoding node_private_key"),
		).unwrap();
		let secp = Secp256k1::new();
		let verifying_key = secret_key.public_key(&secp);
		let vote_sign_payload = VoteSignPayload {
			block_number: 1,
			block_hash: [1; 32],
			cluster_address: [0; 20],
			vote: true,
			epoch: 1,
		};

		let sig = vote_sign_payload.sign_with_ecdsa(secret_key).unwrap();
		let vote = Vote {
			data: vote_sign_payload,
			validator_address: [1; 20],
			signature: sig.serialize_compact().to_vec(),
			verifying_key: verifying_key.serialize().to_vec(),
		};

		let new_block_header = BlockHeader {
			block_number: 1,
			block_hash: [1; 32],
			parent_hash: [0; 32],
			block_type: BlockType::L1XTokenBlock,
			cluster_address: [0; 20],
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: [0; 32],
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		assert!(validate_vote_fields(&vote, &block).await.is_ok());
	}

	#[tokio::test]
	async fn test_validate_validate_vote_fields_for_incorrect_hash() {
		let node_private_key = "f6b82b53ecbe1978b8651f740739b1d181f0285381e65e5e3491d8e821ab9bd0";
		let secret_key = SecretKey::from_slice(
			&hex::decode(node_private_key).expect("Error decoding node_private_key"),
		).unwrap();
		let secp = Secp256k1::new();
		let verifying_key = secret_key.public_key(&secp);
		let vote_sign_payload = VoteSignPayload {
			block_number: 1,
			block_hash: [1; 32],
			cluster_address: [0; 20],
			vote: true,
			epoch: 1,
		};

		let sig = vote_sign_payload.sign_with_ecdsa(secret_key).unwrap();
		let vote = Vote {
			data: vote_sign_payload,
			validator_address: [1; 20],
			signature: sig.serialize_compact().to_vec(),
			verifying_key: verifying_key.serialize().to_vec(),
		};

		let new_block_header = BlockHeader {
			block_number: 1,
			block_hash: [2; 32],
			parent_hash: [0; 32],
			block_type: BlockType::L1XTokenBlock,
			cluster_address: [0; 20],
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: [0; 32],
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let result = validate_vote_fields(&vote, &block).await;
		assert!(result.is_err());
		if let Err(error) = result {
			assert!(error.to_string().contains("Incorrect block hash"));
		}
	}

	#[tokio::test]
	async fn test_validate_validate_vote_fields_for_incorrect_signature() {
		let node_private_key = "f6b82b53ecbe1978b8651f740739b1d181f0285381e65e5e3491d8e821ab9bd0";
		let private_key = "f2b82b53ecbe1978b8651f740739b1d181f0285381e65e5e3491d8e821ab9bd0";
		let secret_key = SecretKey::from_slice(
			&hex::decode(node_private_key).expect("Error decoding node_private_key"),
		).unwrap();
		let other_secret_key = SecretKey::from_slice(
			&hex::decode(private_key).expect("Error decoding node_private_key"),
		).unwrap();
		let secp = Secp256k1::new();
		let verifying_key = secret_key.public_key(&secp);
		let vote_sign_payload = VoteSignPayload {
			block_number: 1,
			block_hash: [1; 32],
			cluster_address: [0; 20],
			vote: true,
			epoch: 1,
		};

		let sig = vote_sign_payload.sign_with_ecdsa(other_secret_key).unwrap();
		let vote = Vote {
			data: vote_sign_payload,
			validator_address: [1; 20],
			signature: sig.serialize_compact().to_vec(),
			verifying_key: verifying_key.serialize().to_vec(),
		};

		let new_block_header = BlockHeader {
			block_number: 1,
			block_hash: [1; 32],
			parent_hash: [0; 32],
			block_type: BlockType::L1XTokenBlock,
			cluster_address: [0; 20],
			timestamp: 1,
			num_transactions: 0,
			block_version: 1,
			state_hash: [0; 32],
			epoch: 1,
		};
		let block = Block {
			block_header: new_block_header,
			transactions: vec![],
		};

		let result = validate_vote_fields(&vote, &block).await;
		assert!(result.is_err());
		if let Err(error) = result {
			assert!(error.to_string().contains("Vote type verify_signature()"));
		}

	}
}
