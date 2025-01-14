-- Your SQL goes here
CREATE INDEX IF NOT EXISTS idx_block_header_block_hash ON block_header(block_hash);