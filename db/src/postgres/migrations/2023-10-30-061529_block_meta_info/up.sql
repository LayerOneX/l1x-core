-- Your SQL goes here
CREATE TABLE IF NOT EXISTS block_meta_info (
    cluster_address VARCHAR,
    block_number Numeric,
    block_executed boolean,
    PRIMARY KEY (cluster_address, block_number)
)