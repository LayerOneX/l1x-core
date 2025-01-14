use anyhow::Error;
use primitives::Address;
use system::transaction::Transaction;
use compile_time_config::config::MINT_MASTER_ADDRESS;

pub fn is_mint(from_address: &Address, to_address: &Address) -> Result<bool, Error> {
	if from_address == to_address && from_address == &MINT_MASTER_ADDRESS {
		Ok(true)
	} else {
		Ok(false)
	}
}

pub fn is_mint_tx(from_address: &Address, tx: &Transaction) -> Result<bool, Error> {
	match tx.transaction_type {
		system::transaction::TransactionType::NativeTokenTransfer(to_address, _balance) =>
			is_mint(from_address, &to_address),
		_ => Ok(false),
	}
}
