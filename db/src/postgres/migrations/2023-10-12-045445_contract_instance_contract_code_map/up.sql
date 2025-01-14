-- Your SQL goes here
CREATE TABLE IF NOT EXISTS contract_instance_contract_code_map (
    instance_address VARCHAR,
    contract_address VARCHAR,
    owner_address VARCHAR,
    PRIMARY KEY (instance_address, contract_address)
)