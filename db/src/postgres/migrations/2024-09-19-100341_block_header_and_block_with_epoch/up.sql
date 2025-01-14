-- Your SQL goes here
-- Step 1: Add the new column 'epoch' if it doesn't exist
ALTER TABLE block_header ADD COLUMN IF NOT EXISTS epoch NUMERIC;

-- Step 2: Update the 'epoch' column in batches to avoid locking the entire table
DO $$
DECLARE
    batch_size INT := 100000; -- Batch size for updates
    rows_updated INT;         -- Number of rows updated in each batch
BEGIN
    LOOP
        WITH cte AS (
                    SELECT ctid
                    FROM block_header
                    WHERE epoch IS NULL
                    LIMIT batch_size
                )
                UPDATE block_header
                SET epoch = 1
                WHERE ctid IN (SELECT ctid FROM cte);

                GET DIAGNOSTICS rows_updated = ROW_COUNT; -- Get the number of rows updated

        -- Exit the loop if no more rows were updated
        IF rows_updated = 0 THEN
            EXIT;
        END IF;
    END LOOP;
END $$;

-- Step 3: Create the index on the 'epoch' column after updating
CREATE INDEX IF NOT EXISTS idx_block_header_epoch ON block_header (epoch);
CREATE INDEX IF NOT EXISTS idx_block_header_block_version ON block_header (block_version);
CREATE INDEX IF NOT EXISTS idx_block_header_state_hash ON block_header (state_hash);
