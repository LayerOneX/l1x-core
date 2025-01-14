-- Your SQL goes here
CREATE TABLE IF NOT EXISTS contract (
    address VARCHAR PRIMARY KEY,
    access smallint,
    code VARCHAR,
    owner_address VARCHAR,
    type smallint
)