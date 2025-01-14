use anyhow::anyhow;
use std::path::PathBuf;

use l1x_consensus_primitives::Address as L1xAddress;

pub fn derive_code_address_from(contract_instance_address: L1xAddress) -> L1xAddress {
	let mut contract_code_address = contract_instance_address.clone();
	contract_code_address[0] = contract_code_address[0].wrapping_add(13u8);
	contract_code_address
}

pub fn generate_l1x_address_from_path(path: &PathBuf) -> anyhow::Result<L1xAddress> {
	let mut path_bytes = path
		.to_str()
		.ok_or(anyhow!("Can't convert the path to bytes: {path:?}"))?
		.as_bytes()
		.to_vec();
	let empty_addr = L1xAddress::default();

	path_bytes.extend(empty_addr);

	path_bytes[..empty_addr.len()]
		.try_into()
		.map_err(|_| anyhow!("Can't create l1x address from the path {path:?}"))
}
