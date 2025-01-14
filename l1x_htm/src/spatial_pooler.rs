use ndarray::{Array1, Array2};
use rand::Rng;

#[derive(Debug, Clone)]
pub struct SpatialPooler {
	input_size: usize,
	output_size: usize,
	potential_pool: Array2<bool>,
	permanences: Array2<f64>,
	connected_synapses: Array2<bool>,
	active_duty_cycles: Array1<f64>,
	boost_factors: Array1<f64>,
}

impl SpatialPooler {
	pub fn new(input_size: usize, output_size: usize, potential_pct: f64) -> Self {
		let mut rng = rand::thread_rng();
		let potential_pool = Array2::from_shape_fn((output_size, input_size), |_| rng.gen::<f64>() < potential_pct);
		let permanences = Array2::from_shape_fn((output_size, input_size), |_| rng.gen());
		let connected_synapses = permanences.mapv(|x| x > 0.2);

		SpatialPooler {
			input_size,
			output_size,
			potential_pool,
			permanences,
			connected_synapses,
			active_duty_cycles: Array1::zeros(output_size),
			boost_factors: Array1::ones(output_size),
		}
	}

	pub fn compute(&mut self, input: &[bool]) -> Vec<bool> {
		let overlap = self.calculate_overlap(input);
		let active_columns = self.inhibit_columns(&overlap);
		self.update_duty_cycles(&active_columns);
		self.update_boost_factors();
		self.update_permanences(input, &active_columns);
		active_columns
	}

	fn calculate_overlap(&self, input: &[bool]) -> Array1<f64> {
		let input_array = Array1::from_vec(input.iter().map(|&x| if x { 1.0 } else { 0.0 }).collect());
		let overlap = self.connected_synapses.map(|&x| if x { 1.0 } else { 0.0 })
			.dot(&input_array);
		overlap * &self.boost_factors
	}

	fn inhibit_columns(&self, overlap: &Array1<f64>) -> Vec<bool> {
		// Implement k-winners-take-all inhibition
		let k = (0.02 * self.output_size as f64) as usize;
		let mut active_columns = vec![false; self.output_size];
		let mut indices: Vec<usize> = (0..self.output_size).collect();
		
		// Custom sorting function to handle NaN values
		indices.sort_by(|&a, &b| {
			match overlap[b].partial_cmp(&overlap[a]) {
				Some(ordering) => ordering,
				None => {
					// If either value is NaN, consider them equal
					std::cmp::Ordering::Equal
				}
			}
		});
	
		for &i in indices.iter().take(k) {
			active_columns[i] = true;
		}
		active_columns
	}

	fn update_duty_cycles(&mut self, active_columns: &[bool]) {
		let period = 1000.0;
		let active_array = Array1::from_vec(active_columns.iter().map(|&x| if x { 1.0 } else { 0.0 }).collect());
		self.active_duty_cycles = &self.active_duty_cycles * (period - 1.0) / period
			+ &active_array / period;
	}

	fn update_boost_factors(&mut self) {
		let min_duty_cycle = 0.01;
		self.boost_factors = self.active_duty_cycles.mapv(|x| if x > min_duty_cycle { 1.0 } else { min_duty_cycle / x });
	}

	fn update_permanences(&mut self, input: &[bool], active_columns: &[bool]) {
		let learning_rate = 0.1;
		for (i, &active) in active_columns.iter().enumerate() {
			if active {
				for j in 0..self.input_size {
					if self.potential_pool[[i, j]] {
						let delta = if input[j] { learning_rate } else { -learning_rate };
						self.permanences[[i, j]] = (self.permanences[[i, j]] + delta).clamp(0.0, 1.0);
						self.connected_synapses[[i, j]] = self.permanences[[i, j]] > 0.2;
					}
				}
			}
		}
	}
}