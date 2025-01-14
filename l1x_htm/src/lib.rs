mod spatial_pooler;
mod temporal_memory;
mod encoder;
mod anomaly_likelihood;
pub mod realtime_checks;
mod utils;

use std::collections::HashMap;
use system::{block::Block, transaction::Transaction};
use spatial_pooler::SpatialPooler;
use temporal_memory::TemporalMemory;
use encoder::ScalarEncoder;
use anomaly_likelihood::AnomalyLikelihood;
use utils::compute_anomaly;

pub const INVALID_RESPONSE_TIME: u64 = u64::MAX;

#[derive(Debug, Clone)]
pub struct HTM {
	spatial_poolers: HashMap<String, SpatialPooler>,
	temporal_memory: TemporalMemory,
	anomaly_likelihood: AnomalyLikelihood,
	encoders: HashMap<String, ScalarEncoder>,
	current_anomaly_score: f64,
}

impl HTM {
	pub fn new() -> Self {
		let mut spatial_poolers = HashMap::new();
		let mut encoders = HashMap::new();

		for feature in &["transaction_volume", "transaction_frequency", "fee_limit", "block_size"] {
			let encoder = ScalarEncoder::new(0.0, 100.0, 50, 10);
			let spatial_pooler = SpatialPooler::new(encoder.output_width(), 2048, 0.02);
			encoders.insert(feature.to_string(), encoder);
			spatial_poolers.insert(feature.to_string(), spatial_pooler);
		}

		HTM {
			spatial_poolers,
			temporal_memory: TemporalMemory::new(2048, 16),
			anomaly_likelihood: AnomalyLikelihood::new(),
			encoders,
			current_anomaly_score: 0.0,
		}
	}

	pub fn process_transaction(&mut self, transaction: &Transaction) {
		let features = self.extract_transaction_features(transaction);
		self.process_features(features);
	}

	pub fn process_block(&mut self, block: &Block) {
		let features = self.extract_block_features(block);
		self.process_features(features);
	}

	fn process_features(&mut self, features: HashMap<String, f64>) {
		let mut combined_sdr = vec![false; 2048];

		for (feature, value) in features {
			let encoder = self.encoders.get_mut(&feature).unwrap();
			let spatial_pooler = self.spatial_poolers.get_mut(&feature).unwrap();

			let encoded = encoder.encode(value);
			let sdr = spatial_pooler.compute(&encoded);

			for (i, &active) in sdr.iter().enumerate() {
				combined_sdr[i] |= active;
			}
		}

		let (active_cells, predictive_cells) = self.temporal_memory.compute(&combined_sdr, true);
		
		// Convert active_cells and predictive_cells to boolean arrays
		let active_columns: Vec<bool> = (0..2048).map(|i| active_cells.contains(&i)).collect();
		let predicted_columns: Vec<bool> = (0..2048).map(|i| predictive_cells.contains(&i)).collect();

		let anomaly_score = compute_anomaly(&active_columns, &predicted_columns);
		self.current_anomaly_score = self.anomaly_likelihood.compute(anomaly_score);
	}

	pub fn get_anomaly_score(&self) -> f64 {
		self.current_anomaly_score
	}

	fn extract_transaction_features(&self, transaction: &Transaction) -> HashMap<String, f64> {
		let mut features = HashMap::new();
		features.insert("transaction_volume".to_string(), transaction.fee_limit as f64);
		features.insert("fee_limit".to_string(), transaction.fee_limit as f64);
		features
	}

	fn extract_block_features(&self, block: &Block) -> HashMap<String, f64> {
		let mut features = HashMap::new();
		features.insert("block_size".to_string(), block.transactions.len() as f64);
		features.insert("transaction_frequency".to_string(), block.transactions.len() as f64);
		features
	}
}