-- Your SQL goes here
CREATE TABLE IF NOT EXISTS  contract_instance (
    instance_address VARCHAR,
    key VARCHAR,
    value VARCHAR,
    PRIMARY KEY (instance_address, key)
)