-- Your SQL goes here
CREATE TABLE IF NOT EXISTS transaction_metadata (
    transaction_hash VARCHAR PRIMARY KEY,
    is_successful boolean
)