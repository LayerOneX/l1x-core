-- This file should undo anything in `up.sql`
ALTER TABLE block_proposer
  DROP COLUMN epoch;

ALTER TABLE block_proposer
  ADD COLUMN block_number Numeric;
