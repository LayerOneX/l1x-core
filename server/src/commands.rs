pub mod init;
pub mod apply_snapshot;
pub mod start;
pub mod update_node_info;
pub mod add_system_block;
use crate::commands::{
	init::InitCmd, start::StartCmd, update_node_info::UpdateNodeInfoCmd, apply_snapshot::ApplySnapshotCmd, add_system_block::AddSystemBlockCmd
};
use async_trait::async_trait;
use structopt::StructOpt;

#[async_trait]
pub trait L1XCommand {
	/// Returns the result of the command execution.
	async fn execute(self);
}

#[derive(Debug, StructOpt)]
pub enum Command {
	///Initialize the l1x-core genesis file, config and node
	#[structopt(name = "init")]
	Init(InitCmd),
	///Apply snapshot
	#[structopt(name = "apply_snapshot")]
	ApplySnapshot(ApplySnapshotCmd),
	///Start the l1x-core
	#[structopt(name = "start")]
	Start(StartCmd),
	///Update Node Info in DB
	#[structopt(name = "update-node-info")]
	UpdateNodeInfo(UpdateNodeInfoCmd),
	///Adds a system block
	#[structopt(name = "add-system-block")]
	AddSystemBlock(AddSystemBlockCmd),
}

impl Command {
	/// Wrapper around `StructOpt::from_args` method.
	pub fn from_args() -> Self {
		<Self as StructOpt>::from_args()
	}
}

#[async_trait]
impl L1XCommand for Command {
	async fn execute(self) {
		match self {
			Self::Init(command) => command.execute().await,
			Self::ApplySnapshot(command) => command.execute().await,
			Self::Start(command) => command.execute().await,
			Self::UpdateNodeInfo(command) => command.execute().await,
			Self::AddSystemBlock(command) => command.execute().await,
		}
	}
}
