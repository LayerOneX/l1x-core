-- Your SQL goes here
CREATE INDEX IF NOT EXISTS idx_transaction_hash_timestamp ON block_transaction(transaction_hash, timestamp DESC);