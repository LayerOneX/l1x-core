use anyhow::Error;
use primitives::Epoch;
use system::node_health::NodeHealth;
use std::collections::HashMap;
use system::block::Block;
use crate::{HTM, INVALID_RESPONSE_TIME};

pub struct RealTimeChecks {
	pub online_checks: HashMap<String, Vec<(bool, u64)>>,
	pub node_healths: HashMap<String, NodeHealth>,
	pub eligible_peers: (Epoch, Vec<String>),
	pub htm: HTM,
}

impl RealTimeChecks {
	pub fn new() -> Self {
		Self { online_checks: HashMap::new(), node_healths: HashMap::new(), eligible_peers: (0, vec![]), htm: HTM::new() } // TODO: This is for demo purposes.Use an appropriate window and min_history size
	}

	pub fn add_check(&mut self, peer_id: String, is_online: bool, response_time: u64) {
		self.online_checks.entry(peer_id).or_insert_with(Vec::new).push((is_online, response_time));
	}

	pub fn update_eligible_peers(&mut self, epoch: Epoch, peer_ids: Vec<String>) {
		if epoch == 0 && self.eligible_peers.1.is_empty() {
			self.eligible_peers = (epoch, peer_ids);
		} else if self.eligible_peers.0 < epoch {
			self.eligible_peers = (epoch, peer_ids);
		}
	}

	pub fn create_health_report(&mut self, measured_peer_id: String, measuring_peer_id: String, epoch: Epoch, joined_epoch: Epoch) -> Result<NodeHealth, Error> {
		let checks = self
			.online_checks
			.get(&measured_peer_id)
			.filter(|checks| !checks.is_empty())
			.ok_or_else(|| anyhow::anyhow!("No online checks available for peer"))?;

		let uptime_percentage = checks.iter()
			.filter(|(is_online, _)| *is_online)
			.count() as f64 / checks.len() as f64 * 100.0;

		// Calculate average response time, excluding invalid response times
		let valid_response_times: Vec<u64> = checks.iter()
			.filter_map(|(_, rt)| (*rt != INVALID_RESPONSE_TIME).then(|| *rt))
			.collect();
		let average_response_time = if !valid_response_times.is_empty() {
			valid_response_times.iter().sum::<u64>() / valid_response_times.len() as u64
		} else {
			INVALID_RESPONSE_TIME // Default value when there are no valid response times
		};

		let mut node_health = NodeHealth::new(
			measured_peer_id.clone(),
			measuring_peer_id,
			epoch,
			joined_epoch,
			uptime_percentage,
			average_response_time,
		);

		if let Some(existing_health) = self.node_healths.get(&measured_peer_id) {
			node_health.transaction_count = existing_health.transaction_count;
			node_health.block_proposal_count = existing_health.block_proposal_count;
		}

		let input = vec![
			uptime_percentage,
			average_response_time as f64,
			node_health.transaction_count as f64,
			node_health.block_proposal_count as f64,
		];
		log::debug!("Raw input for HTM: {:?}", input);

		let normalized_input = self.normalize_input(input);
		log::debug!("Normalized input for HTM: {:?}", normalized_input);
		node_health.anomaly_score = self.htm.get_anomaly_score();

		self.node_healths.insert(measured_peer_id, node_health.clone());

		Ok(node_health)
	}

	pub fn clear_checks(&mut self) {
		self.online_checks.clear();
	}

	pub fn update_node_health_incrementally(&mut self, measure_peer_id: &str, peer_id: &str, joined_epoch: Epoch, block: &Block) {
		if let Some(health) = self.node_healths.get_mut(measure_peer_id) {
			health.update_incrementally(block);
		} else {
			let mut new_health = NodeHealth::new(
				measure_peer_id.to_string(),
				peer_id.to_string(),
				block.block_header.epoch,
				joined_epoch,
				100.0,
				0
			);
			new_health.update_incrementally(block);
			self.node_healths.insert(measure_peer_id.to_string(), new_health);
		}
	}

	// pub fn update_node_health(&mut self, peer_id: &str, metrics: Vec<f64>) {
	// 	if let Some(anomaly_score) = self.htm.compute(metrics) {
	// 		if let Some(health) = self.node_healths.get_mut(peer_id) {
	// 			health.update_anomaly_score(anomaly_score);
	// 			log::debug!("Updated security measure score for peer {}: {}", peer_id, health.anomaly_score);
	// 		}
	// 	}
	// }

	fn normalize_input(&self, input: Vec<f64>) -> Vec<f64> {
		let expected_ranges = vec![
			(0.0, 100.0),     // uptime percentage
			(0.0, 1000.0),    // average response time (ms)
			(0.0, 1000000.0), // transaction count
			(0.0, 1000.0),    // block proposal count
		];

		input.iter().zip(expected_ranges.iter())
			.map(|(&value, &(min, max))| {
				if min == max {
					0.5 // If min equals max, return 0.5 to avoid division by zero
				} else {
					(value - min) / (max - min)
				}
			})
			.collect()
	}
}
