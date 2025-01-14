-- Your SQL goes here
CREATE TABLE IF NOT EXISTS staking_pool (
    pool_address VARCHAR NOT NULL PRIMARY KEY,
    cluster_address VARCHAR,
    contract_instance_address VARCHAR,
    created_block_number Numeric,
    max_pool_balance Numeric,
    max_stake Numeric,
    min_pool_balance Numeric,
    min_stake Numeric,
    pool_owner VARCHAR,
    staking_period VARCHAR,
    updated_block_number Numeric
)