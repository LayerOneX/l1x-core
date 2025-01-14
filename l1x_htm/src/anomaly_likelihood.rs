use std::collections::VecDeque;
use statrs::distribution::Normal;
use statrs::distribution::ContinuousCDF;

#[derive(Debug, Clone)]
pub struct AnomalyLikelihood {
	distribution: Normal,
	running_avg: f64,
	running_std: f64,
	window_size: usize,
	history: VecDeque<f64>,
}

impl AnomalyLikelihood {
	pub fn new() -> Self {
		AnomalyLikelihood {
			distribution: Normal::new(0.0, 1.0).unwrap(),
			running_avg: 0.0,
			running_std: 1.0,
			window_size: 1000,
			history: VecDeque::new(),
		}
	}

	pub fn compute(&mut self, anomaly_score: f64) -> f64 {
		self.history.push_back(anomaly_score);
		if self.history.len() > self.window_size {
			self.history.pop_front();
		}

		if self.history.len() < 10 {
			return 0.5;
		}

		let (new_avg, new_std) = self.update_running_stats();
		self.running_avg = new_avg;
		self.running_std = new_std;

		let likelihoods: Vec<f64> = self.history.iter()
			.map(|&score| self.distribution.cdf((score - self.running_avg) / self.running_std))
			.collect();

		likelihoods.iter().sum::<f64>() / likelihoods.len() as f64
	}

	fn update_running_stats(&self) -> (f64, f64) {
		let n = self.history.len() as f64;
		let sum: f64 = self.history.iter().sum();
		let mean = sum / n;
		let variance = self.history.iter()
			.map(|&x| (x - mean).powi(2))
			.sum::<f64>() / n;
		(mean, variance.sqrt())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_anomaly_likelihood() {
		let mut al = AnomalyLikelihood::new();
		
		// Test initial behavior
		assert_eq!(al.compute(0.1), 0.5);
		
		// Fill the history
		for i in 0..1000 {
			al.compute(i as f64 / 1000.0);
		}
		
		// Test normal value
		let normal_likelihood = al.compute(0.5);
		assert!(normal_likelihood > 0.4 && normal_likelihood < 0.6);
		
		// Test anomaly
		let anomaly_likelihood = al.compute(2.0);
		assert!(anomaly_likelihood > 0.9);
	}
}