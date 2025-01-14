extern crate structopt;

use serde::{Deserialize, Serialize};
use structopt::StructOpt;

#[derive(Debug, Clone, Serialize, Deserialize, StructOpt)]
pub struct Config {
	#[structopt(long = "postgres-url", env = "POSTGRES_URL")]
	pub db_url: String,
	#[structopt(long = "postgres-pool-size", env = "POSTGRES_POOL_SIZE", default_value = "3")]
	pub pool_size: u32,
	#[structopt(
		long = "postgres-username",
		env = "POSTGRES_USER_NAME",
		default_value = "postgres"
	)]
	pub postgres_username: String,
	#[structopt(
		long = "ppostgres-password",
		env = "POSTGRES_PASSWORD",
		default_value = "postgres"
	)]
	pub postgres_password: String,
	#[structopt(long = "ppostgres-db-name", env = "POSTGRES_DB_NAME", default_value = "l1x")]
	pub postgres_db_name: String,
	#[structopt(long = "dev-mode", env = "DEV_MODE")]
	pub dev_mode: bool,
}

impl Config {
	// Function to generate a Config for test environment
	pub fn test_config() -> Self {
		Self {
			db_url: "postgres://postgres:postgres@localhost:5432/test_db".to_string(),
			pool_size: 10,
			postgres_username: "postgres".to_string(),
			postgres_password: "postgres".to_string(),
			postgres_db_name: "l1x_test".to_string(),
			dev_mode: true,
		}
	}

	// Function to generate a Config for local environment
	pub fn local_config() -> Self {
		Self {
			db_url: "postgres://postgres:postgres@localhost:5432/l1x".to_string(),
			pool_size: 30,
			postgres_username: "postgres".to_string(),
			postgres_password: "postgres".to_string(),
			postgres_db_name: "l1x".to_string(),
			dev_mode: false,
		}
	}
}
