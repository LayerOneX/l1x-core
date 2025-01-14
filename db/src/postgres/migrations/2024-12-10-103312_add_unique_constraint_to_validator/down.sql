-- This file should undo anything in `up.sql`
ALTER TABLE validator
DROP CONSTRAINT IF EXISTS unique_address_epoch;
