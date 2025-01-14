-- Your SQL goes here
CREATE TABLE IF NOT EXISTS block_transaction (
    transaction_hash VARCHAR,
    block_number Numeric,
    block_hash VARCHAR,
    fee_used Numeric,
    from_address VARCHAR,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    transaction bytea,
    tx_sequence Numeric,
    PRIMARY KEY (transaction_hash, block_number)
)