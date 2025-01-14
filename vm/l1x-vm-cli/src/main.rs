mod helpers;
mod trait_vm_impl;
use hex;
use l1x_consensus_primitives::{
	Address as L1xAddress, Balance, BlockHash, BlockNumber, BlockTimeStamp, Gas
};
use l1x_consensus_traits::traits_l1xvm::VMContractTrait;
use l1x_rbpf::runtime_config::RuntimeGas;
use l1x_vm_execution_fee::execution_fees::VmExecutionFees;
use std::{
	path::PathBuf,
	rc::Rc,
	sync::{Arc, Mutex},
};
use trait_vm_impl::{BlockchainEnv, CachedDB, VmApi};

use clap::Parser;
use l1x_ebpf_runtime::{run_with_registers, verify_ebpf};
use std::cell::RefCell;

const DEFAULT_CALLER_ACCOUNT_ID: &str = "ee00000000000000000000000000000000000000";
const DEFAULT_CURRENT_ACCOUNT_ID: &str = "ff00000000000000000000000000000000000000";
const DB_PATH: &str = "l1x.db";

#[derive(Parser, Debug)]
#[clap(version)]
enum Cli {
	#[clap(about = "Run eBPF VM")]
	Run {
		#[clap(help = "Path to an eBPF smart contract binary")]
		ebpf_path: PathBuf,
		#[clap(help = "The name of the smart contract function which should be executed")]
		fn_name: String,
		#[clap(
			help = "Input arguments in JSON format. These arguments are passed to the function"
		)]
		input: Option<String>,
		#[clap(long)]
		#[clap(help = "Sets a caller account address")]
		caller_address: Option<String>,
		#[clap(long)]
		#[clap(
			help = "Sets a contract code address. If it's not set, the address is derived from the contract instance address"
		)]
		contract_code_address: Option<String>,
		#[clap(long)]
		#[clap(
			help = "Sets an owner account address. This is the account address of the owner of the called contract"
		)]
		contract_owner_address: Option<String>,
		#[clap(long)]
		#[clap(
			help = "Sets a contract instance address. If it's not set, the contract file name is used"
		)]
		contract_instance_address: Option<String>,
		#[clap(long)]
		#[clap(help = "Inits VM refisters with the provided values")]
		register_args: Option<Vec<u64>>,
		#[clap(long)]
		#[clap(
			help = "Emulates read-only/view method call. In this case the called method will not be able to change the contract state"
		)]
		read_only: bool,
		#[clap(long)]
		#[clap(help = "Sets Gas limit")]
		#[arg(default_value_t = Gas::MAX)]
		gas: Gas,
		#[clap(long)]
		#[clap(help = "Sets Deposit")]
		#[arg(default_value_t = 0)]
		deposit: Balance,
	},
	#[clap(about = "Verify eBPF object file")]
	Verify {
		#[clap(help = "Path to an eBPF smart contract binary")]
		ebpf_path: PathBuf,
	},
}

fn to_l1x_address(id: &str) -> anyhow::Result<L1xAddress> {
	match hex::decode(id) {
		Ok(addr) => match <L1xAddress>::try_from(addr.clone()) {
			Ok(v) => Ok(v),
			Err(_) => Err(anyhow::anyhow!(
				"Can't parse L1X address '{}', incorrect length {} bytes",
				id,
				addr.len()
			)),
		},
		Err(e) => Err(anyhow::anyhow!("Can't parse L1X address '{}', error {}", id, e.to_string())),
	}
}

fn create_vm_api<'a>(
	contract_path: &PathBuf,
	block_hash: BlockHash,
	block_number: BlockNumber,
	block_timestamp: BlockTimeStamp,
	gas_limit: u64,
) -> VmApi {
	let cached_db = CachedDB::new(DB_PATH);

	let env = BlockchainEnv {
		block_hash,
		block_number,
		block_timestamp,
		runtime_gas: Rc::new(RefCell::new(RuntimeGas::new(gas_limit))),
	};

	let vm_api = VmApi::new(contract_path, Arc::new(Mutex::new(cached_db)), env);

	vm_api
}

fn main() -> anyhow::Result<()> {
	env_logger::init();
	let (
		ebpf_path,
		fn_name,
		input,
		caller_address,
		contract_code_address,
		contract_owner_address,
		contract_instance_address,
		register_args,
		read_only,
		gas_limit,
		deposit,
	) = match Cli::parse() {
		Cli::Run {
			ebpf_path,
			fn_name,
			input,
			caller_address,
			contract_code_address,
			contract_owner_address,
			contract_instance_address,
			register_args,
			read_only,
			gas,
			deposit,
		} => (
			ebpf_path,
			fn_name,
			input,
			caller_address,
			contract_code_address,
			contract_owner_address,
			contract_instance_address,
			register_args,
			read_only,
			gas,
			deposit,
		),
		Cli::Verify { ebpf_path } => {
			let elf_bytes = std::fs::read(ebpf_path)?;
			verify_ebpf(&elf_bytes)?;
			return Ok(())
		},
	};

	let block_number: BlockNumber = 42;
	let block_hash: BlockHash = [
		1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
		25, 26, 27, 28, 29, 30, 31, 32,
	];
	let block_timestamp: BlockTimeStamp = 0xff;

	let mut vm_api =
		create_vm_api(&ebpf_path, block_hash, block_number, block_timestamp, gas_limit);
	let vm_api_ref: Arc<RefCell<&mut dyn VMContractTrait>> = Arc::new(RefCell::new(&mut vm_api));
	let input = input.unwrap_or("{}".to_string());
	let caller_address = caller_address.unwrap_or(DEFAULT_CALLER_ACCOUNT_ID.to_string());
	let contract_owner_address =
		contract_owner_address.unwrap_or(DEFAULT_CURRENT_ACCOUNT_ID.to_string());

	let elf_bytes = std::fs::read(ebpf_path.clone())?;

	let caller_address = to_l1x_address(&caller_address)?;
	let contract_owner_address = to_l1x_address(&contract_owner_address)?;
	let contract_instance_address = match contract_instance_address {
		Some(v) => to_l1x_address(&v)?,
		None => helpers::generate_l1x_address_from_path(&ebpf_path)?,
	};
	let contract_code_address = match contract_code_address {
		Some(v) => to_l1x_address(&v)?,
		None => helpers::derive_code_address_from(contract_instance_address),
	};

	match run_with_registers(
		&elf_bytes,
		fn_name.into_bytes(),
		input.into_bytes(),
		caller_address,
		contract_code_address,
		contract_instance_address,
		contract_owner_address,
		block_number,
		block_hash,
		block_timestamp,
		read_only,
		gas_limit,
		deposit,
		VmExecutionFees::create_basic_config(),
		vm_api_ref,
		register_args,
	) {
		Ok(return_data) => {
			vm_api.commit_changes();
			println!("{:?}", return_data)
		},
		Err(e) => eprintln!("{}", e.to_string()),
	}

	Ok(())
}
