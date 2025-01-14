-- This file should undo anything in `up.sql`
ALTER TABLE block_header
  DROP COLUMN block_version,
  DROP COLUMN state_hash;