-- Your SQL goes here
CREATE INDEX IF NOT EXISTS idx_block_transaction_from_address ON block_transaction (from_address);