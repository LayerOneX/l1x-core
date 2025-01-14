-- Your SQL goes here
CREATE TABLE IF NOT EXISTS vote (
    block_hash VARCHAR,
    block_number Numeric,
    cluster_address VARCHAR,
    validator_address VARCHAR,
    signature VARCHAR,
    verifying_key VARCHAR,
    voting boolean,
    PRIMARY KEY (block_hash, validator_address)
)