
#[derive(Debug, Clone)]
pub struct ScalarEncoder {
	min_val: f64,
	max_val: f64,
	num_buckets: usize,
	output_width: usize,
}

impl ScalarEncoder {
	pub fn new(min_val: f64, max_val: f64, num_buckets: usize, output_width: usize) -> Self {
		ScalarEncoder {
			min_val,
			max_val,
			num_buckets,
			output_width,
		}
	}

	pub fn encode(&self, value: f64) -> Vec<bool> {
		let bucket = ((value - self.min_val) / (self.max_val - self.min_val) * self.num_buckets as f64).floor() as usize;
		let mut encoding = vec![false; self.output_width];
		let start = bucket * self.output_width / self.num_buckets;
		let end = ((bucket + 1) * self.output_width / self.num_buckets).min(self.output_width);
		for i in start..end {
			encoding[i] = true;
		}
		encoding
	}

	pub fn output_width(&self) -> usize {
		self.output_width
	}
}