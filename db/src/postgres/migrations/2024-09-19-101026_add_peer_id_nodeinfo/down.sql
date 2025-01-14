-- This file should undo anything in `up.sql`
DROP INDEX idx_node_info_address;
ALTER TABLE node_info
  DROP COLUMN peer_id,
  DROP COLUMN joined_epoch;
