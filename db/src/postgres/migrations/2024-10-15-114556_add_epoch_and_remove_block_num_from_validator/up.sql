-- Your SQL goes here
ALTER TABLE validator
  ADD COLUMN epoch Numeric;

-- Create an index on 'block_number' to speed up the maximum lookup
CREATE INDEX IF NOT EXISTS idx_validator_block_number ON validator (block_number);


WITH max_block_number AS (
  SELECT MAX(block_number) AS max_block_number
  FROM validator
)
UPDATE validator
SET epoch = CASE
  WHEN block_number = (SELECT max_block_number FROM max_block_number)
         THEN FLOOR(block_number / 100) -- 100 is SLOTS_PER_EPOCH
    ELSE 0
END;

-- Drop the index on 'block_number' (no longer needed)
DROP INDEX IF EXISTS idx_validator_block_number;

ALTER TABLE validator
  DROP COLUMN block_number;

CREATE INDEX IF NOT EXISTS idx_validator_epoch ON validator (epoch);