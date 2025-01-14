pub mod eth {
	pub mod block;
	pub mod bloom_tree;
	pub mod bytes;
	pub mod filter;
	pub mod log;
	pub mod signers;
	pub mod sync;
	pub mod transaction;
	pub mod transaction_id;
}
use crate::eth::bytes::Bytes;
use ethereum::TransactionV2 as EthereumTransaction;
use ethereum_types::H160;
use serde::{de::Error, Deserialize, Deserializer};
#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize)]
pub(crate) struct CallOrInputData {
	data: Option<Bytes>,
	input: Option<Bytes>,
}

#[allow(dead_code)]
/// Function to deserialize `data` and `input`  within `TransactionRequest` and `CallRequest`.
/// It verifies that if both `data` and `input` are provided, they must be identical.
pub(crate) fn deserialize_data_or_input<'d, D: Deserializer<'d>>(
	d: D,
) -> Result<Option<Bytes>, D::Error> {
	let CallOrInputData { data, input } = CallOrInputData::deserialize(d)?;
	match (&data, &input) {
		(Some(data), Some(input)) =>
			if data == input {
				Ok(Some(data.clone()))
			} else {
				Err(D::Error::custom("Ambiguous value for `data` and `input`".to_string()))
			},
		(_, _) => Ok(data.or(input)),
	}
}

/// The trait that used to build types from the `from` address and ethereum `transaction`.
pub trait BuildFrom {
	fn build_from(from: H160, transaction: &EthereumTransaction) -> Self;
}

