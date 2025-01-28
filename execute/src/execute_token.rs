use anyhow::Error;
use log::info;
use mint::mint::is_mint;
use primitives::*;

use state::UpdatedState;

pub struct ExecuteToken {}

impl ExecuteToken {
	pub fn execute_native_token_transfer<'a>(
		from_address: &Address,
		to_address: &Address,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error> {
		info!("DEBUB::TMP_27012025: Executing native token transfer from: {:?} to: {:?} amount: {}", hex::encode(from_address), hex::encode(to_address), amount);
		if is_mint(from_address, to_address)? {
			info!("Minting {} tokens for address: {:?}", amount, hex::encode(to_address));
			updated_state.mint(amount, to_address)?;
		} else {
			updated_state.transfer(&from_address, &to_address, amount)?;
		}
		Ok(())
	}

	pub fn just_transfer_tokens<'a>(
		from_address: &Address,
		to_address: &Address,
		amount: Balance,
		updated_state: &mut UpdatedState,
	) -> Result<(), Error> {
		updated_state.transfer(&from_address, &to_address, amount)?;

		Ok(())
	}
}
