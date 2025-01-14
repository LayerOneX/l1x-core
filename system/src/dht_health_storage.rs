use libp2p::kad::{
		record::Key, Kademlia, record::store::MemoryStore, 
		QueryId, QueryResult, 
		Quorum, Record,
		GetRecordOk
};
use primitives::Epoch;
use crate::node_health::NodeHealth;
use anyhow::Error;
use std::collections::HashMap;
use tokio::sync::oneshot;


pub struct DHTHealthStorage{
	pub pending_get_requests: HashMap<QueryId, oneshot::Sender<Result<NodeHealth, Error>>>,
}

impl DHTHealthStorage{
	pub fn new() -> Self {
		Self {
			pending_get_requests: HashMap::new(),
		}
	}

	pub async fn store_node_health(&mut self, kademlia: &mut Kademlia<MemoryStore>, node_health: &NodeHealth) -> Result<QueryId, Error> {
		let key = Key::new(&format!("{}:{}", node_health.measured_peer_id, node_health.epoch).into_bytes());
		let value = serde_json::to_vec(&node_health)?;
		let record = Record {
			key,
			value,
			publisher: None,
			expires: None,
		};
		let query_id = kademlia.put_record(record, Quorum::One)?;
		Ok(query_id)
	}

	pub fn get_node_health(&mut self, kademlia: &mut Kademlia<MemoryStore>, peer_id: String, epoch: Epoch) -> oneshot::Receiver<Result<NodeHealth, Error>> {
		log::info!("fetching node health for peer: {}", peer_id);
		let key = Key::new(&format!("{}:{}", peer_id, epoch).into_bytes());
		let (sender, receiver) = oneshot::channel();
		let query_id = kademlia.get_record(key);
		self.pending_get_requests.insert(query_id, sender);
		receiver
	}

    pub fn handle_get_record_result(&mut self, query_id: QueryId, result: QueryResult) {
        if let Some(sender) = self.pending_get_requests.remove(&query_id) {
            let result = match result {
                QueryResult::GetRecord(Ok(ok)) => {
                    match ok {
                        GetRecordOk::FoundRecord(record) => {
                            match serde_json::from_slice(&record.record.value) {
                                Ok(node_health) => Ok(node_health),
                                Err(e) => Err(anyhow::anyhow!("Failed to deserialize NodeHealth: {}", e)),
                            }
                        },
                        GetRecordOk::FinishedWithNoAdditionalRecord { .. } => {
                            Err(anyhow::anyhow!("No record found"))
                        }
                    }
                }
                _ => Err(anyhow::anyhow!("Unexpected query result")),
            };
            let _ = sender.send(result);
        }
    }
}