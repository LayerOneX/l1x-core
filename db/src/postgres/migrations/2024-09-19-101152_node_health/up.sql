-- Your SQL goes here
CREATE TABLE node_health (
	measured_peer_id VARCHAR NOT NULL,
	peer_id VARCHAR NOT NULL,
	epoch NUMERIC NOT NULL,
	joined_epoch NUMERIC NOT NULL,
	uptime_percentage DOUBLE PRECISION NOT NULL,
	response_time_ms NUMERIC NOT NULL,
	transaction_count NUMERIC NOT NULL,
	block_proposal_count NUMERIC NOT NULL,
	anomaly_score DOUBLE PRECISION NOT NULL,
	node_health_version INTEGER NOT NULL,
	PRIMARY KEY (measured_peer_id, peer_id, epoch)
);