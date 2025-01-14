// https://github.com/polkadot-evm/frontier/blob/master/client/rpc/src/eth/filter.rs
use super::{block::BlockNumber, bloom_tree::keccak256, log::Log};
use ethereum_types::{Bloom, BloomInput, H160, H256, U256};
use serde::{
	de::{DeserializeOwned, Error},
	Deserialize, Deserializer,
};
use serde_json::{from_value, Value};

/// Subscription kind.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
	/// New block headers subscription.
	NewHeads,
	/// Logs subscription.
	Logs,
	/// New Pending Transactions subscription.
	NewPendingTransactions,
	/// Node syncing status subscription.
	Syncing,
}

/// Subscription kind.
#[derive(Clone, Debug, Eq, PartialEq, Default, Hash)]
pub enum Params {
	/// No parameters passed.
	#[default]
	None,
	/// Log parameters.
	Logs(Filter),
}

impl<'a> Deserialize<'a> for Params {
	fn deserialize<D>(deserializer: D) -> ::std::result::Result<Params, D::Error>
	where
		D: Deserializer<'a>,
	{
		let v: Value = Deserialize::deserialize(deserializer)?;

		if v.is_null() {
			return Ok(Params::None)
		}

		from_value(v)
			.map(Params::Logs)
			.map_err(|e| D::Error::custom(format!("Invalid Pub-Sub parameters: {}", e)))
	}
}

/// Variadic value
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub enum VariadicValue<T>
where
	T: DeserializeOwned,
{
	/// Single
	Single(T),
	/// List
	Multiple(Vec<T>),
	/// None
	#[default]
	Null,
}

impl From<H160> for VariadicValue<H160> {
	fn from(src: H160) -> Self {
		VariadicValue::Single(src)
	}
}

impl From<Vec<H160>> for VariadicValue<H160> {
	fn from(src: Vec<H160>) -> Self {
		VariadicValue::Multiple(src)
	}
}

impl From<H256> for Topic {
	fn from(src: H256) -> Self {
		VariadicValue::Single(Some(src))
	}
}

impl From<Vec<H256>> for VariadicValue<H256> {
	fn from(src: Vec<H256>) -> Self {
		VariadicValue::Multiple(src)
	}
}

impl From<VariadicValue<H256>> for Topic {
	fn from(src: VariadicValue<H256>) -> Self {
		match src {
			VariadicValue::Single(val) => VariadicValue::Single(Some(val)),
			VariadicValue::Multiple(arr) => arr.into(),
			VariadicValue::Null => VariadicValue::Single(None),
		}
	}
}

impl<I: Into<H256>> From<Vec<I>> for Topic {
	fn from(src: Vec<I>) -> Self {
		VariadicValue::Multiple(src.into_iter().map(Into::into).map(Some).collect())
	}
}

impl From<H160> for Topic {
	fn from(src: H160) -> Self {
		let mut bytes = [0; 32];
		bytes[12..32].copy_from_slice(src.as_bytes());
		VariadicValue::Single(Some(H256::from(bytes)))
	}
}

impl From<U256> for Topic {
	fn from(src: U256) -> Self {
		let mut bytes = [0; 32];
		src.to_big_endian(&mut bytes);
		VariadicValue::Single(Some(H256::from(bytes)))
	}
}

impl<T: DeserializeOwned> VariadicValue<T> {
	pub fn to_opt_vec(self) -> Option<Vec<T>> {
		match self {
			VariadicValue::Single(v) => Some(vec![v]),
			VariadicValue::Multiple(v) => Some(v),
			VariadicValue::Null => None,
		}
	}
}

impl<'a, T> Deserialize<'a> for VariadicValue<T>
where
	T: DeserializeOwned,
{
	fn deserialize<D>(deserializer: D) -> Result<VariadicValue<T>, D::Error>
	where
		D: Deserializer<'a>,
	{
		let v: Value = Deserialize::deserialize(deserializer)?;

		if v.is_null() {
			return Ok(VariadicValue::Null)
		}

		from_value(v.clone())
			.map(VariadicValue::Single)
			.or_else(|_| from_value(v).map(VariadicValue::Multiple))
			.map_err(|err| D::Error::custom(format!("Invalid variadic value type: {}", err)))
	}
}

/// Filter Address
pub type FilterAddress = VariadicValue<H160>;
/// Topic, supports `A` | `null` | `[A,B,C]` | `[A,[B,C]]` | `[null,[B,C]]` | `[null,[null,C]]`
pub type Topic = VariadicValue<Option<H256>>;
/// FlatTopic, simplifies the matching logic.
pub type FlatTopic = VariadicValue<Option<H256>>;
pub type BloomFilter<'a> = Vec<Option<Bloom>>;

impl From<&VariadicValue<H160>> for Vec<Option<Bloom>> {
	fn from(address: &VariadicValue<H160>) -> Self {
		let mut blooms = BloomFilter::new();
		match address {
			VariadicValue::Single(address) => {
				let bloom: Bloom = BloomInput::Raw(address.as_ref()).into();
				blooms.push(Some(bloom))
			},
			VariadicValue::Multiple(addresses) =>
				if addresses.is_empty() {
					blooms.push(None);
				} else {
					for address in addresses.iter() {
						let bloom: Bloom = BloomInput::Raw(address.as_ref()).into();
						blooms.push(Some(bloom));
					}
				},
			_ => blooms.push(None),
		}
		blooms
	}
}

impl From<&VariadicValue<Option<H256>>> for Vec<Option<Bloom>> {
	fn from(topics: &VariadicValue<Option<H256>>) -> Self {
		let mut blooms = BloomFilter::new();
		match topics {
			VariadicValue::Single(topic) =>
				if let Some(topic) = topic {
					let bloom: Bloom = BloomInput::Raw(topic.as_ref()).into();
					blooms.push(Some(bloom));
				} else {
					blooms.push(None);
				},
			VariadicValue::Multiple(topics) =>
				if topics.is_empty() {
					blooms.push(None);
				} else {
					for topic in topics.iter() {
						if let Some(topic) = topic {
							let bloom: Bloom = BloomInput::Raw(topic.as_ref()).into();
							blooms.push(Some(bloom));
						} else {
							blooms.push(None);
						}
					}
				},
			_ => blooms.push(None),
		}
		blooms
	}
}

/// Filter
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
	/// From Block
	pub from_block: Option<BlockNumber>,
	/// To Block
	pub to_block: Option<BlockNumber>,
	/// Block hash
	pub block_hash: Option<H256>,
	/// Address
	pub address: FilterAddress,
	/// Topics
	/// TODO: use TopicFilter
	pub topics: [Option<Topic>; 4],
}

impl Filter {
	pub fn new() -> Self {
		Self::default()
	}

	#[allow(clippy::wrong_self_convention)]
	#[must_use]
	pub fn from_block<T: Into<BlockNumber>>(mut self, block: T) -> Self {
		self.from_block = Some(block.into());
		self
	}

	#[allow(clippy::wrong_self_convention)]
	#[must_use]
	pub fn to_block<T: Into<BlockNumber>>(mut self, block: T) -> Self {
		self.to_block = Some(block.into());
		self
	}

	#[allow(clippy::wrong_self_convention)]
	#[must_use]
	pub fn at_block_hash<T: Into<H256>>(mut self, hash: T) -> Self {
		self.block_hash = Some(hash.into());
		self
	}
	#[must_use]
	pub fn address<T: Into<FilterAddress>>(mut self, address: T) -> Self {
		self.address = address.into();
		self
	}

	/// Given the event signature in string form, it hashes it and adds it to the topics to monitor
	#[must_use]
	pub fn event(self, event_name: &str) -> Self {
		let hash = keccak256(event_name.as_bytes());
		self.topic0(hash)
	}

	/// Hashes all event signatures and sets them as array to topic0
	#[must_use]
	pub fn events(self, events: impl IntoIterator<Item = impl AsRef<[u8]>>) -> Self {
		let events = events.into_iter().map(|e| keccak256(e.as_ref())).collect::<Vec<_>>();
		self.topic0(events)
	}

	/// Sets topic0 (the event name for non-anonymous events)
	#[must_use]
	pub fn topic0<T: Into<Topic>>(mut self, topic: T) -> Self {
		self.topics[0] = Some(topic.into());
		self
	}

	/// Sets the 1st indexed topic
	#[must_use]
	pub fn topic1<T: Into<Topic>>(mut self, topic: T) -> Self {
		self.topics[1] = Some(topic.into());
		self
	}

	/// Sets the 2nd indexed topic
	#[must_use]
	pub fn topic2<T: Into<Topic>>(mut self, topic: T) -> Self {
		self.topics[2] = Some(topic.into());
		self
	}

	/// Sets the 3rd indexed topic
	#[must_use]
	pub fn topic3<T: Into<Topic>>(mut self, topic: T) -> Self {
		self.topics[3] = Some(topic.into());
		self
	}

	/// Cartesian product for VariadicValue conditional indexed parameters.
	/// Executed once on struct instance.
	/// i.e. `[A,[B,C]]` to `[[A,B],[A,C]]`.
	/// Flattens the topics using the cartesian product
	fn flatten(&self) -> Vec<VariadicValue<Option<H256>>> {
		fn cartesian(lists: &[Vec<Option<H256>>]) -> Vec<Vec<Option<H256>>> {
			let mut res = Vec::new();
			let mut list_iter = lists.iter();
			if let Some(first_list) = list_iter.next() {
				for &i in first_list {
					res.push(vec![i]);
				}
			}
			for l in list_iter {
				let mut tmp = Vec::new();
				for r in res {
					for &el in l {
						let mut tmp_el = r.clone();
						tmp_el.push(el);
						tmp.push(tmp_el);
					}
				}
				res = tmp;
			}
			res
		}
		let mut out = Vec::new();
		let mut tmp = Vec::new();
		for v in self.topics.iter() {
			let v = if let Some(v) = v {
				match v {
					VariadicValue::Single(s) => {
						vec![*s]
					},
					VariadicValue::Multiple(s) => s.clone(),
					VariadicValue::Null => vec![None],
				}
			} else {
				vec![None]
			};
			tmp.push(v);
		}
		for v in cartesian(&tmp) {
			out.push(VariadicValue::Multiple(v));
		}
		out
	}

	/// Returns an iterator over all existing topics
	pub fn topics(&self) -> impl Iterator<Item = &Topic> + '_ {
		self.topics.iter().flatten()
	}

	/// Returns true if at least one topic is set
	pub fn has_topics(&self) -> bool {
		self.topics.iter().any(|t| t.is_some())
	}
}
#[derive(Debug, Default)]
pub struct FilteredParams {
	pub filter: Option<Filter>,
	pub flat_topics: Vec<FlatTopic>,
}

impl FilteredParams {
	pub fn new(filter: Option<Filter>) -> Self {
		if let Some(filter) = filter {
			let flat_topics = filter.flatten();
			FilteredParams { filter: Some(filter), flat_topics }
		} else {
			Self::default()
		}
	}

	/// Build an address-based BloomFilter.
	pub fn adresses_bloom_filter(address: &Option<FilterAddress>) -> BloomFilter<'_> {
		if let Some(address) = address {
			return address.into();
		}
		Vec::new()
	}

	/// Build a topic-based BloomFilter.
	pub fn topics_bloom_filter(topics: &Option<Vec<FlatTopic>>) -> Vec<BloomFilter<'_>> {
		let mut output: Vec<BloomFilter> = Vec::new();
		if let Some(topics) = topics {
			for flat in topics {
				output.push(flat.into());
			}
		}
		output
	}
	/// Evaluates if a Bloom contains a provided sequence of topics.
	pub fn topics_in_bloom(bloom: Bloom, topic_bloom_filters: &[BloomFilter]) -> bool {
		if topic_bloom_filters.is_empty() {
			// No filter provided, match.
			return true;
		}
		// A logical OR evaluation over `topic_bloom_filters`.
		for subset in topic_bloom_filters.iter() {
			let mut matches = false;
			for el in subset {
				matches = match el {
					Some(input) => bloom.contains_bloom(input),
					// Wildcards are true.
					None => true,
				};
				// Each subset must be evaluated sequentially to true or break.
				if !matches {
					break;
				}
			}
			// If any subset is fully evaluated to true, there is no further evaluation.
			if matches {
				return true;
			}
		}
		false
	}

	/// Evaluates if a Bloom contains the provided address(es).
	pub fn address_in_bloom(bloom: Bloom, address_bloom_filter: &BloomFilter) -> bool {
		if address_bloom_filter.is_empty() {
			// No filter provided, match.
			return true;
		} else {
			// Wildcards are true.
			for el in address_bloom_filter {
				if match el {
					Some(input) => bloom.contains_bloom(input),
					None => true,
				} {
					return true;
				}
			}
		}
		false
	}

	/// Replace None values - aka wildcards - for the log input value in that position.
	pub fn replace(&self, log: &Log, topic: FlatTopic) -> Option<Vec<H256>> {
		let mut out: Vec<H256> = Vec::new();
		match topic {
			VariadicValue::Single(Some(value)) => {
				out.push(value);
			},
			VariadicValue::Multiple(value) =>
				for (k, v) in value.into_iter().enumerate() {
					if let Some(v) = v {
						out.push(v);
					} else {
						out.push(log.topics[k]);
					}
				},
			_ => {},
		};
		if out.is_empty() {
			return None;
		}
		Some(out)
	}

	pub fn filter_block_range(&self, block_number: u64) -> bool {
		let mut out = true;
		let filter = self.filter.clone().unwrap();
		if let Some(BlockNumber::Num(from)) = filter.from_block {
			if from > block_number {
				out = false;
			}
		}
		if let Some(to) = filter.to_block {
			match to {
				BlockNumber::Num(to) =>
					if to < block_number {
						out = false;
					},
				BlockNumber::Earliest => {
					out = false;
				},
				_ => {},
			}
		}
		out
	}

	pub fn filter_block_hash(&self, block_hash: H256) -> bool {
		if let Some(h) = self.filter.clone().unwrap().block_hash {
			if h != block_hash {
				return false;
			}
		}
		true
	}

	pub fn filter_address(&self, log: &Log) -> bool {
		if let Some(filter) = self.filter.as_ref() {
			match &filter.address {
				VariadicValue::Single(x) =>
					if log.address != *x {
						return false;
					},
				VariadicValue::Multiple(x) =>
					if !x.contains(&log.address) {
						return false;
					},
				_ => (),
			}
		}

		true
	}

	pub fn filter_topics(&self, log: &Log) -> bool {
		let mut out: bool = true;
		for topic in self.flat_topics.clone() {
			match topic {
				VariadicValue::Single(single) =>
					if let Some(single) = single {
						if !log.topics.starts_with(&[single]) {
							out = false;
						}
					},
				VariadicValue::Multiple(multi) => {
					// Shrink the topics until the last item is Some.
					let mut new_multi = multi;
					while new_multi.iter().last().unwrap_or(&Some(H256::default())).is_none() {
						new_multi.pop();
					}
					// We can discard right away any logs with lesser topics than the filter.
					if new_multi.len() > log.topics.len() {
						out = false;
						break;
					}
					let replaced: Option<Vec<H256>> =
						self.replace(log, VariadicValue::Multiple(new_multi));
					if let Some(replaced) = replaced {
						out = false;
						if log.topics.starts_with(&replaced[..]) {
							out = true;
							break;
						}
					}
				},
				_ => {
					out = true;
				},
			}
		}
		out
	}
}
