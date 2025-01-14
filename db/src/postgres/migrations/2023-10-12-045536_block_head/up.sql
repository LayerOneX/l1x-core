-- Your SQL goes here
CREATE TABLE IF NOT EXISTS block_head (
    cluster_address VARCHAR NOT NULL PRIMARY KEY,
    block_number Numeric,
    block_hash VARCHAR
)