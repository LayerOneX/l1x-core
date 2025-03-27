use primitives::BlockNumber;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDetailedStatus {
    pub peer_id: String,
    pub current_block: BlockNumber,
    pub pending_transactions: u32,
    pub connected_peers: u32,
    pub uptime_seconds: u64,
} 