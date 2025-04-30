use std::error::Error;
use crate::{
	block::{BlockPayload, L1xResponse, QueryBlockMessage}, block_proposer::BlockProposerPayload, node_health::NodeHealthPayload, node_status::NodeDetailedStatus, transaction::Transaction, validator::ValidatorPayload, vote::Vote, vote_result::VoteResult
};
use primitive_types::{H160, H256};
use primitives::*;

use crate::node_info::NodeInfo;
use serde::{Deserialize, Serialize};
use libp2p::{request_response::ResponseChannel, PeerId};
use tokio::sync::oneshot;
use crate::node_health::NodeHealth;

#[derive(Debug)]
pub enum BroadcastNetwork {
	BroadcastNodeInfo(NodeInfo),
	BroadcastTransaction(Transaction),
	BroadcastValidateBlock(BlockPayload),
	BroadcastBlockProposer(BlockProposerPayload),
	BroadcastValidators(ValidatorPayload),
	BroadcastVote(Vote),
	BroadcastVoteResult(VoteResult),
	BroadcastQueryBlockRequest(QueryBlockMessage, PeerId),
	BroadcastQueryBlockResponse(L1xResponse, ResponseChannel<L1xResponse>),
	BroadcastSignedNodeHealth(Vec<NodeHealthPayload>),
	BroadcastNodeHealth(Vec<NodeHealth>),
	BroadcastQueryNodeStatusRequest(TimeStamp, PeerId),
	BroadcastNodeDetailedStatus(NodeDetailedStatus)
}

#[derive(Debug)]
pub enum NetworkMessage {
	NetworkEvent(NetworkEventType),
	BlockProposerEvent(BlockProposerEventType),
}

#[derive(Debug)]
pub enum NetworkEventType {
	ReceiveNodeInfo(NodeInfo),
	ReceiveValidateBlock(BlockPayload),
	ReceiveBlockProposer(BlockProposerPayload),
	ReceiveValidators(ValidatorPayload),
	ReceiveVote(Vote),
	ReceiveVoteResult(VoteResult),
	ReceiveQueryBlockResponse(BlockPayload, Option<VoteResult>),
	ReceiveQueryBlockRequest(QueryBlockMessage, ResponseChannel<L1xResponse>),
	AggregateNodeHealth(Epoch),
	ProcessNodeHealth(Epoch),
	ReceiveNodeHealth(Vec<NodeHealth>),
    ReceiveAggregatedNodeHealth(Vec<NodeHealthPayload>),
	PingResult(String, bool, u64), // peer_id, is_success, rtt
	PingEligiblePeers(Epoch, Vec<String>),
	CheckNodeStatus,
	PublishNodeDetailedStatus(String),
	ReceiveNodeDetailedStatus(NodeDetailedStatus),
}

#[derive(Debug)]
pub enum BlockProposerEventType {
	AddBlock(BlockPayload, oneshot::Sender<Result<NetworkAcknowledgement, Box<dyn Error + Send>>>),
	AddNewBlockProposer(BlockProposerPayload, oneshot::Sender<Result<NetworkAcknowledgement, Box<dyn Error + Send>>>),
	AddNewValidators(ValidatorPayload, oneshot::Sender<Result<NetworkAcknowledgement, Box<dyn Error + Send>>>),
}

#[derive(Debug, Clone)]
pub enum NetworkAcknowledgement {
	Success,
	Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventBroadcast {
	Evm(
		H160,
		Vec<H256>,
		Vec<u8>,
		BlockNumber,
		BlockHash,
		TransactionHash,
		u64, // txn_index
		u64, // log_index
	),
}
