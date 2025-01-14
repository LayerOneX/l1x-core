-- This file should undo anything in `up.sql`
DROP INDEX idx_block_header_block_version;
DROP INDEX idx_block_header_state_hash;

ALTER TABLE block_header
  DROP COLUMN epoch;