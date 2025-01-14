use anyhow::Result;
use plonky2::field::goldilocks_field::GoldilocksField;
use plonky2::iop::witness::{PartialWitness, WitnessWrite};
use plonky2::plonk::circuit_builder::CircuitBuilder;
use plonky2::plonk::circuit_data::{CircuitConfig, CircuitData};
use plonky2::plonk::config::PoseidonGoldilocksConfig;
use plonky2::plonk::proof::ProofWithPublicInputs;
use plonky2::plonk::prover::prove;
use plonky2::util::timing::TimingTree;

pub struct ZkpManager {
	circuit_data: CircuitData<GoldilocksField, PoseidonGoldilocksConfig, 2>,
}

impl ZkpManager {
	pub fn new() -> Self {
		let config = CircuitConfig::standard_recursion_config();
		let mut builder = CircuitBuilder::<GoldilocksField, 2>::new(config);
		// Define your circuit here
		let data = builder.build::<PoseidonGoldilocksConfig>();

		Self {
			circuit_data: data,
		}
	}

	pub fn create_proof(&self, inputs: &[GoldilocksField]) -> Result<ProofWithPublicInputs<GoldilocksField, PoseidonGoldilocksConfig, 2>> {
		let mut pw = PartialWitness::new();
		for (i, &input) in inputs.iter().enumerate() {
			let target = self.circuit_data.prover_only.public_inputs[i];
			pw.set_target(target, input);
		}
		let mut timing = TimingTree::default();
		let proof = prove(&self.circuit_data.prover_only, &self.circuit_data.common, pw, &mut timing)?;
		Ok(proof)
	}

	pub fn verify_proof(&self, proof: ProofWithPublicInputs<GoldilocksField, PoseidonGoldilocksConfig, 2>) -> Result<()> {
		self.circuit_data.verify(proof)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use plonky2::field::types::Field;
	use plonky2::plonk::config::{GenericConfig, PoseidonGoldilocksConfig};

	#[test]
	fn test_create_and_verify_proof() {
		const D: usize = 2;
		type C = PoseidonGoldilocksConfig;
		type F = <C as GenericConfig<D>>::F;

		let manager = ZkpManager::new();

		let inputs = vec![F::ONE, F::TWO];
		let proof = manager.create_proof(&inputs).unwrap();
		let result = manager.verify_proof(proof);
		assert!(result.is_ok());
	}
}