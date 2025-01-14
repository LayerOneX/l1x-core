-- Your SQL goes here
-- Step 1: Delete duplicate rows, keeping only one record for each (address, epoch) combination
DELETE FROM validator
WHERE ctid NOT IN (
    SELECT MIN(ctid)
    FROM validator
    GROUP BY address, epoch
);

-- Step 2: Add the unique constraint
ALTER TABLE validator
ADD CONSTRAINT unique_address_epoch UNIQUE (address, epoch);
