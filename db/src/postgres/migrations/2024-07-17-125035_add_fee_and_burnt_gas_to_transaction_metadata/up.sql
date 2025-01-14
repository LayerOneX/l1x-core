-- Your SQL goes here
ALTER TABLE transaction_metadata
ADD COLUMN fee NUMERIC DEFAULT 0,
ADD COLUMN burnt_gas NUMERIC DEFAULT 0;