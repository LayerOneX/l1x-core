-- Your SQL goes here
CREATE TABLE IF NOT EXISTS block_header (
     block_number Numeric NOT NULL PRIMARY KEY,
     block_hash VARCHAR,
     parent_hash VARCHAR,
     timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
     block_type smallint,
     num_transactions Integer,
     cluster_address VARCHAR
)