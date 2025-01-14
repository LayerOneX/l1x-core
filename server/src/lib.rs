pub const SNAPSHOT_FILE: &str = "snapshot.json";
pub const SNAPSHOT_RANGE_FILE: &str = "snapshot_range.json";

pub mod grpc;
pub mod json;
pub mod service;
pub mod snapshot;
pub mod commands {
	pub mod start;
}
pub mod traits {
	pub mod eth;
	pub mod l1x;
}