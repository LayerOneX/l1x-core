use std::fmt;

use anyhow::{anyhow, Error, Result};
use primitive_types::H256;
use primitives::{EventType as EventTypei8, *};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
	pub transaction_hash: TransactionHash,
	pub event_data: EventData,
	pub block_number: BlockNumber,
	pub event_type: EventTypei8,
	pub contract_address: Address,
	pub topics: Option<Vec<H256>>,
}

impl Event {
	pub fn new(
		transaction_hash: TransactionHash,
		event_data: EventData,
		block_number: BlockNumber,
		event_type: EventTypei8,
		contract_address: Address,
		topics: Option<Vec<H256>>,
	) -> Event {
		Event { transaction_hash, event_data, block_number, event_type, contract_address, topics }
	}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventType {
	L1XVM,
	EVM,
}

impl TryInto<EventType> for i8 {
	type Error = Error;

	fn try_into(self) -> Result<EventType, Self::Error> {
		match self {
			0 => Ok(EventType::L1XVM),
			1 => Ok(EventType::EVM),
			_ => Err(anyhow!("Invalid contract type {}", self)),
		}
	}
}

impl fmt::Display for Event {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let topics_str = if let Some(topics) = &self.topics {
			Some(topics.iter().map(|t|{
				format!("{:?},", t)
			}).collect::<Vec<_>>())
		} else {
			None
		};

		write!(f, "Event {{transaction_hash: {}, event_data: {}, block_number: {}, event_type: {}, contract_address: {}, topics: [{:?}]}}",
			hex::encode(&self.transaction_hash), hex::encode(&self.event_data), self.block_number, self.event_type, hex::encode(&self.contract_address), topics_str)
	}
}