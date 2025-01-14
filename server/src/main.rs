extern crate core;

pub mod commands;

use crate::commands::Command;
pub mod grpc;
pub mod json;
pub mod service;
pub mod traits;

pub mod snapshot;

#[tokio::main]
async fn main() {
	match Command::from_args() {
		Command::Start(cmd) => cmd.execute().await,
		Command::ApplySnapshot(cmd) => cmd.execute().await,
		Command::Init(cmd) => cmd.execute().await,
		Command::UpdateNodeInfo(cmd) => cmd.execute().await,
		Command::AddSystemBlock(cmd) => cmd.execute().await,
	}
}
