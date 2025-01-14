-- Your SQL goes here
CREATE TABLE IF NOT EXISTS validator (
    address VARCHAR,
    cluster_address VARCHAR,
    block_number Numeric,
    stake Numeric,
    PRIMARY KEY (address, block_number)
)