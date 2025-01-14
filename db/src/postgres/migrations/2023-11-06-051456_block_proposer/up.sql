-- Your SQL goes here
CREATE TABLE IF NOT EXISTS block_proposer (
    address  VARCHAR,
    cluster_address VARCHAR,
    block_number Numeric,
    selected_next Boolean,
    PRIMARY KEY (address,cluster_address,block_number)
)