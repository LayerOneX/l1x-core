-- Your SQL goes here
CREATE TABLE IF NOT EXISTS node_info (
      address VARCHAR NOT NULL PRIMARY KEY,
      cluster_address VARCHAR,
      ip_address VARCHAR,
	metadata VARCHAR,
      signature VARCHAR,
      verifying_key VARCHAR
)