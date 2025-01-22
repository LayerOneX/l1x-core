use std::collections::HashMap;
use std::time::{Duration, Instant};
use compile_time_config::HEALTH_VERSION;
use libp2p_gossipsub::MessageId;
use primitives::{Epoch, Address, SignatureBytes, VerifyingKeyBytes};
use serde::{Serialize, Deserialize};
use l1x_vrf::{
	common::{get_signature_from_bytes, SecpVRF},
	secp_vrf::KeySpace,
};
use crate::block::Block;
use std::error::Error;
use async_trait::async_trait;
use std::ops::{Add, Div};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeHealthPayload {
    pub node_health: NodeHealth,
    pub signature: SignatureBytes,
    pub verifying_key: VerifyingKeyBytes,
	// This is a workaround to make the broadcast block_proposer message unique for each node
	pub sender: Address,
}

impl NodeHealthPayload {
	pub async fn verify_signature(&self) -> Result<(), anyhow::Error> {
		let signature = get_signature_from_bytes(&self.signature.to_vec())?;
		let public_key = KeySpace::public_key_from_bytes(&self.verifying_key)?;
		self.node_health.verify_with_ecdsa(&public_key, signature)
			.map_err(|e| anyhow::anyhow!("NodeInfo: {}", e))
	}
	
	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match serde_json::to_vec(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}
}

#[derive(Clone, Serialize, Debug, Deserialize, Default)]
pub struct NodeHealth {
	pub measured_peer_id: String,
	pub peer_id: String,
	pub epoch: Epoch,
	pub joined_epoch: Epoch,
	pub uptime_percentage: f64,
	pub response_time_ms: u64,
	pub transaction_count: u64,
	pub block_proposal_count: u64,
	pub anomaly_score: f64,
	pub node_health_version: u32,
}

impl NodeHealth {
	pub fn new(measured_peer_id: String, peer_id: String, epoch: Epoch, joined_epoch: Epoch, uptime_percentage: f64, response_time_ms: u64) -> Self {
		Self { 
			measured_peer_id, 
			peer_id, 
			epoch, 
			joined_epoch, 
			uptime_percentage, 
			response_time_ms, 
			transaction_count: 0, 
			block_proposal_count: 0,
			anomaly_score: 0.0, 
			node_health_version: HEALTH_VERSION,
		}
	}

	pub fn update_incrementally(&mut self, block: &Block) {
		self.transaction_count += block.transactions.len() as u64;
		self.block_proposal_count += 1;
	}

	pub fn update_anomaly_score(&mut self, anomaly_score: f64) {
		self.anomaly_score = anomaly_score;
	}

	pub fn aggregate_network_health(node_health_map: HashMap<String, Vec<NodeHealth>>) -> HashMap<String, NodeHealth> {
		node_health_map
			.into_iter()
			.filter_map(|(peer_id, health_reports)| {
				Self::aggregate(health_reports).map(|health| (peer_id, health))
			})
			.collect()
	}

	fn aggregate(health_reports: Vec<NodeHealth>) -> Option<NodeHealth> {
		let report_count = health_reports.len();

		if report_count == 0 {
			return None;
		}

		// Initialize lists to store values for median calculation
		let mut uptime_percentages: Vec<f64> = Vec::with_capacity(report_count);
		let mut response_times: Vec<u64> = Vec::with_capacity(report_count);
		let mut anomaly_scores: Vec<f64> = Vec::with_capacity(report_count);

		// Initialize aggregated with defaults for `uptime_percentage`, `anomaly_score`, and `response_time_ms`
		let mut aggregated = NodeHealth {
			uptime_percentage: 0.0,
			response_time_ms: 0,
			transaction_count: 0,
			block_proposal_count: 0,
			anomaly_score: 0.0,
			..Default::default() // Retain other default fields
		};

		// Set `measured_peer_id` to the first report's value
		if let Some(first_report) = health_reports.first() {
			aggregated.measured_peer_id = first_report.measured_peer_id.clone();
			aggregated.peer_id = first_report.measured_peer_id.clone();
			aggregated.epoch = first_report.epoch;
		}

		for report in health_reports {
			uptime_percentages.push(report.uptime_percentage);
			response_times.push(report.response_time_ms);
			anomaly_scores.push(report.anomaly_score);

			// Max of transaction count and block proposal count
			aggregated.transaction_count = aggregated.transaction_count.max(report.transaction_count);
			aggregated.block_proposal_count = aggregated.block_proposal_count.max(report.block_proposal_count);
		}

		// Calculate medians for uptime_percentage, response_time_ms, and anomaly_score
		aggregated.uptime_percentage = NodeHealth::median(&mut uptime_percentages);
		aggregated.response_time_ms = NodeHealth::median(&mut response_times);
		aggregated.anomaly_score = NodeHealth::median(&mut anomaly_scores);

		Some(aggregated)
	}

	pub fn as_bytes(&self) -> Result<Vec<u8>, Box<dyn Error + Send>> {
		match serde_json::to_vec(self) {
			Ok(bytes) => Ok(bytes),
			Err(e) => Err(Box::new(e)),
		}
	}

	fn median<T>(values: &mut [T]) -> T
		where
			T: Copy + PartialOrd + Add<Output = T> + Div<Output = T> + From<u8>,
	{
		if values.is_empty() {
			return T::from(0);
		}

		values.sort_by(|a, b| a.partial_cmp(b).unwrap_or_else(|| std::cmp::Ordering::Equal));
		let len = values.len();
		if len % 2 == 0 {
			// If even, calculate the average of the two middle values
			let mid1 = values[len / 2 - 1];
			let mid2 = values[len / 2];
			(mid1 + mid2) / T::from(2)
		} else {
			// If odd, return the middle value
			values[len / 2]
		}
	}
}

#[derive(Clone)]

pub struct CachedNodeHealth {
	pub health: NodeHealth,
	pub last_updated: Instant,
}

#[derive(Clone)]
pub struct HealthCache {
	cache: HashMap<String, CachedNodeHealth>,
	ttl: Duration,
}

impl HealthCache {
	pub fn new(ttl: Duration) -> Self {
		Self { cache: HashMap::new(), ttl }
	}

	pub fn update(&mut self, peer_id: String, health: NodeHealth) {
		self.cache.insert(peer_id, CachedNodeHealth {
			health,
			last_updated: Instant::now(),
		});
	}

	pub fn get(&self, peer_id: &String) -> Option<&NodeHealth> {
		self.cache.get(peer_id).and_then(|cached| {
			if cached.last_updated.elapsed() < self.ttl {
				Some(&cached.health)
			} else {
				None
			}
		})
	}

	pub fn remove_stale(&mut self) {
		self.cache.retain(|_, cached| cached.last_updated.elapsed() < self.ttl);
	}
}


impl PartialEq for NodeHealth {
	fn eq(&self, other: &Self) -> bool {
		self.epoch == other.epoch &&
		self.uptime_percentage.to_bits() == other.uptime_percentage.to_bits() &&
		self.response_time_ms == other.response_time_ms
	}
}

impl Eq for NodeHealth {}

impl PartialOrd for NodeHealth {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(
			self.epoch.cmp(&other.epoch)
				.then_with(|| match self.uptime_percentage.partial_cmp(&other.uptime_percentage) {
					Some(std::cmp::Ordering::Equal) => std::cmp::Ordering::Equal,
					Some(std::cmp::Ordering::Less) => std::cmp::Ordering::Less,
					Some(std::cmp::Ordering::Greater) => std::cmp::Ordering::Greater,
					None => {
						if self.uptime_percentage.is_nan() {
							std::cmp::Ordering::Less
						} else {
							std::cmp::Ordering::Greater
						}
					}
				})
				.then_with(|| self.response_time_ms.cmp(&other.response_time_ms))
		)
	}
}

impl Ord for NodeHealth {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.epoch.cmp(&other.epoch)
			.then_with(|| match self.uptime_percentage.partial_cmp(&other.uptime_percentage) {
				Some(std::cmp::Ordering::Equal) => std::cmp::Ordering::Equal,
				Some(std::cmp::Ordering::Less) => std::cmp::Ordering::Less,
				Some(std::cmp::Ordering::Greater) => std::cmp::Ordering::Greater,
				None => {
					if self.uptime_percentage.is_nan() {
						std::cmp::Ordering::Less
					} else {
						std::cmp::Ordering::Greater
					}
				}
			})
			.then_with(|| self.response_time_ms.cmp(&other.response_time_ms))
	}
}

#[async_trait]
pub trait NodeHealthBroadcast {
	async fn node_health_broadcast(
		&self,
		node_healths: Vec<NodeHealth>,
	) -> Result<MessageId, Box<dyn Error + Send>>;
}

#[async_trait]
pub trait AggregatedNodeHealthBroadcast {
	async fn aggregated_node_health_broadcast(
		&self,
		node_health_payloads: Vec<NodeHealthPayload>,
	) -> Result<MessageId, Box<dyn Error + Send>>;
}
