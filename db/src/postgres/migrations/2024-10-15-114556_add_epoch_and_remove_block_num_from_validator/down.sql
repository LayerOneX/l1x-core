-- This file should undo anything in `up.sql`
ALTER TABLE validator
  ADD COLUMN block_number Numeric;

ALTER TABLE validator
  DROP CONSTRAINT unique_address_epoch;

DROP INDEX IF EXISTS idx_validator_epoch;

ALTER TABLE validator
  DROP COLUMN epoch;
