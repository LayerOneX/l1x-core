use anyhow::{anyhow, Error};
use chrono::NaiveDateTime;
use log::error;
use primitives::{Address, BlockTimeStamp, TimeStamp as PriTimeStamp};
use scylla::frame::value::CqlTimestamp;
use std::{
	fs,
	io,
	path::Path,
	time::{SystemTime, UNIX_EPOCH},
};

pub fn get_cassandra_timestamp() -> Result<CqlTimestamp, Error> {
	let milliseconds: i64 = SystemTime::now()
		.duration_since(UNIX_EPOCH)?
		.as_millis()
		.try_into()
		.unwrap_or(i64::MAX);
	Ok(CqlTimestamp(milliseconds))
}

fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
	for entry in fs::read_dir(&path)? {
		let entry = entry?;
		let path = entry.path();
		if path.is_dir() {
			remove_dir_all(&path)?;
		} else {
			fs::remove_file(&path)?;
		}
	}
	fs::remove_dir(&path)
}

pub fn convert_to_naive_datetime(ts: PriTimeStamp) -> NaiveDateTime {
	/*let duration_in_chrono = ts.0;
	let seconds = duration_in_chrono.num_seconds();
	let nanos = (duration_in_chrono - Duration::seconds(seconds)).num_nanoseconds().unwrap_or(0);*/
	NaiveDateTime::from_timestamp_millis(ts as i64).unwrap_or_default()
}

pub fn block_timestamp_to_naive_datetime(ts: BlockTimeStamp) -> NaiveDateTime {
	/*let duration_in_chrono = ts.0;
	let seconds = duration_in_chrono.num_seconds();
	let nanos = (duration_in_chrono - Duration::seconds(seconds)).num_nanoseconds().unwrap_or(0);*/
	let x = NaiveDateTime::from_timestamp_opt(ts as i64, 0).unwrap_or(NaiveDateTime::default());
	x
}

pub async fn wb_list_check(from_address: &Address, to_address: &Address) -> Result<(), Error> {
	let config = runtime_config::RuntimeDenyConfigCache::get().await?;
	let whitelisted_addresses = &config.whitelisted_addresses;
	let blacklisted_addresses= &config.blacklisted_addresses;

	// Check if transfer is allowed
	let allow_transfer = is_transfer_allowed(whitelisted_addresses, blacklisted_addresses, from_address, to_address);

	if !allow_transfer {
		let message = format!(r#"
			❌❌❌❌❌❌❌❌❌❌❌❌❌❌❌
			👮👮Insufficient privileges 👮👮
			❌❌❌❌❌❌❌❌❌❌❌❌❌❌❌
			From {} to {}
			"#,
			hex::encode(from_address), hex::encode(to_address)
		);
		error!("{}", message);
		return Err(anyhow!(message))
	}

	Ok(())
}

// Function to check transfer is allowed or not
fn is_transfer_allowed(
	whitelisted_addresses: &Option<runtime_config::WBAddresses>,
	blacklisted_addresses: &Option<runtime_config::WBAddresses>,
	from_address: &Address,
	to_address: &Address
) -> bool {
	// Check if sender is blacklisted
	let is_blacklisted_sender = if let Some(blacklisted_addresses) = blacklisted_addresses {
		blacklisted_addresses.sender_addresses.contains(from_address)
	} else {
		false
	};

	// Check if receiver is whitelisted or blacklisted
	let is_whitelisted_receiver = if let Some(whitelisted_addresses) = whitelisted_addresses {
		whitelisted_addresses.receiver_addresses.contains(to_address)
	} else {
		true
	};
	let is_blacklisted_receiver = if let Some(blacklisted_addresses) = blacklisted_addresses {
		blacklisted_addresses.receiver_addresses.contains(to_address)
	} else {
		false
	};

	// Determine if transfer is allowed
	!(is_blacklisted_receiver || (is_blacklisted_sender && !is_whitelisted_receiver))
}

pub fn current_timestamp_in_secs() -> Result<u64, Error> {
	Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

pub fn current_timestamp_in_millis() -> Result<u128, Error> {
	Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())
}

pub fn current_timestamp_in_micros() -> Result<PriTimeStamp, Error> {
	Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros())
}

pub fn seconds_to_milliseconds(seconds: u128) -> u128 {
	seconds * 1000
}

pub fn seconds_to_days(secs: u64) -> u64 {
	let seconds_in_day = 60 * 60 * 24;
	secs / seconds_in_day
}

pub fn milliseconds_to_seconds(msecs: u128) -> u128 {
	msecs / 1000
}

pub fn nanoseconds_to_milliseconds(nanoseconds: u128) -> u128 {
	nanoseconds / 1_000_000
}

#[cfg(test)]
mod tests {
	use std::collections::HashSet;

use super::*;
	use runtime_config::WBAddresses;

	#[test]
	fn test_transfer_allowed_whitelisted_sender_whitelisted_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];
		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::from_iter(vec![sender]),
			receiver_addresses: HashSet::from_iter(vec![receiver]),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::new(),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			true
		);
	}

	#[test]
	fn test_transfer_allowed_whitelisted_sender_blacklisted_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];

		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::from_iter(vec![sender]),
			receiver_addresses: HashSet::new(),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::from_iter(vec![receiver]),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			false
		);
	}

	#[test]
	fn test_transfer_allowed_whitelisted_sender_some_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];

		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::from_iter(vec![sender]),
			receiver_addresses: HashSet::new(),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::new(),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			true
		);
	}

	#[test]
	fn test_transfer_allowed_some_sender_some_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];

		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::new(),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::new(),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			true
		);
	}

	#[test]
	fn test_transfer_allowed_some_sender_whitelisted_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];

		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::from_iter(vec![receiver]),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::new(),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			true
		);
	}

	#[test]
	fn test_transfer_allowed_some_sender_blacklisted_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];

		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::new(),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::from_iter(vec![receiver]),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			false
		);
	}

	#[test]
	fn test_transfer_allowed_blacklisted_sender_some_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];

		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::new(),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::from_iter(vec![sender]),
			receiver_addresses: HashSet::new(),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			false
		);
	}

	#[test]
	fn test_transfer_allowed_blacklisted_sender_whitelisted_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];

		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::from_iter(vec![receiver]),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::from_iter(vec![sender]),
			receiver_addresses: HashSet::new(),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			true
		);
	}

	#[test]
	fn test_transfer_allowed_blacklisted_sender_blacklisted_receiver() {
		let sender = [1; 20];
		let receiver = [2; 20];

		let whitelisted_addresses = WBAddresses {
			sender_addresses: HashSet::new(),
			receiver_addresses: HashSet::new(),
		};
		let blacklisted_addresses = WBAddresses {
			sender_addresses: HashSet::from_iter(vec![sender]),
			receiver_addresses: HashSet::from_iter(vec![receiver]),
		};

		assert_eq!(
			is_transfer_allowed(
				&Some(whitelisted_addresses),
				&Some(blacklisted_addresses),
				&sender,
				&receiver
			),
			false
		);
	}
}



