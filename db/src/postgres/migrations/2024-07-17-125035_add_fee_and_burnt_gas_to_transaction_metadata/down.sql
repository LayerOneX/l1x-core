-- This file should undo anything in `up.sql`
ALTER TABLE transaction_metadata
DROP COLUMN fee,
DROP COLUMN burnt_gas;