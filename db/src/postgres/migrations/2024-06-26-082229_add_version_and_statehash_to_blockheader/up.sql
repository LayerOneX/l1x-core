ALTER TABLE block_header
  ADD COLUMN block_version INT,
  ADD COLUMN state_hash VARCHAR;

UPDATE block_header SET block_version = 1 WHERE block_version IS NULL;