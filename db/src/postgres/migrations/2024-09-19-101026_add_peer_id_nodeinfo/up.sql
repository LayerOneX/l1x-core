-- Your SQL goes here
ALTER TABLE node_info
  ADD COLUMN peer_id VARCHAR,
  ADD COLUMN joined_epoch Numeric;

UPDATE node_info SET joined_epoch = 1;
UPDATE node_info SET peer_id = '';

CREATE INDEX IF NOT EXISTS idx_node_info_address ON node_info(address);
CREATE INDEX IF NOT EXISTS idx_node_info_peer_id ON node_info (peer_id);
CREATE INDEX IF NOT EXISTS idx_node_info_joined_epoch ON node_info (joined_epoch);