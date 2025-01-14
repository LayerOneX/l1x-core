ALTER TABLE block_proposer
  ADD COLUMN epoch Numeric;

ALTER TABLE block_proposer
  DROP COLUMN block_number;

UPDATE block_proposer SET epoch = 1 WHERE epoch IS NULL;

CREATE INDEX IF NOT EXISTS idx_block_proposer_epoch ON block_proposer (epoch);