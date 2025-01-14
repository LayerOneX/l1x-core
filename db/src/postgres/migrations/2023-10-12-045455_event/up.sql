-- Your SQL goes here
CREATE TABLE IF NOT EXISTS event (
    id BIGSERIAL,
    transaction_hash VARCHAR,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    block_number Numeric,
    contract_address VARCHAR,
    event_data VARCHAR,
    event_type smallint,
    topic0 VARCHAR,
    topic1 VARCHAR,
    topic2 VARCHAR,
    topic3 VARCHAR,
    PRIMARY KEY (id)
);
CREATE INDEX ON event (event_type);
CREATE INDEX ON event (contract_address);
CREATE INDEX ON event (topic0);
CREATE INDEX ON event (topic1);
CREATE INDEX ON event (topic2);
CREATE INDEX ON event (topic3);
CREATE INDEX ON event (block_number);
CREATE INDEX ON event (transaction_hash);