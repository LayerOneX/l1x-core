-- Your SQL goes here
CREATE TABLE IF NOT EXISTS staking_account (
    pool_address VARCHAR,
    account_address VARCHAR,
    balance Numeric,
    PRIMARY KEY (pool_address, account_address)
)