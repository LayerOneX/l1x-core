use ethereum_types::{Address, Bloom, H256};
use tiny_keccak::{Hasher, Keccak};

pub fn keccak256(input: &[u8]) -> H256 {
	let mut hasher = Keccak::v256();
	let mut output = [0u8; 32];
	hasher.update(input);
	hasher.finalize(&mut output);
	H256(output)
}

fn add_to_bloom(bloom: &mut Bloom, input: H256) {
	for i in 0..3 {
		let bit_position = ((u32::from(input[i * 2]) << 8) + u32::from(input[i * 2 + 1])) % 2048;
		let byte_position = bit_position / 8;
		let bit = bit_position % 8;
		bloom.0[byte_position as usize] |= 1 << bit;
	}
}

pub fn create_logs_bloom(logs: Vec<(Address, Vec<H256>)>) -> Bloom {
	let mut bloom = Bloom::default();

	for (address, topics) in logs {
		let address_hash = keccak256(address.as_bytes());
		add_to_bloom(&mut bloom, address_hash);

		for topic in topics {
			let topic_hash = keccak256(topic.as_bytes());
			add_to_bloom(&mut bloom, topic_hash);
		}
	}

	bloom
}
