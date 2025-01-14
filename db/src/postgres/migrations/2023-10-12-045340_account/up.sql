-- Create an ENUM type for account_type
CREATE TYPE AccountType AS ENUM ('System', 'User');
-- Create the account table
CREATE TABLE IF NOT EXISTS  account (
     address VARCHAR  PRIMARY KEY,
     account_type AccountType,
     balance Numeric,
     nonce Numeric
)