-- Your SQL goes here
-- Step 1: Add the new column 'xscore' if it doesn't exist
ALTER TABLE validator ADD COLUMN IF NOT EXISTS xscore NUMERIC;

-- Step 2: Update the 'xscore' column in batches to avoid locking the entire table
DO $$
DECLARE
    batch_size INT := 100000;  -- Define the batch size
    rows_updated INT;          -- Tracks the number of rows updated in each batch
BEGIN
    LOOP
        WITH cte AS (
            SELECT ctid
            FROM validator
            WHERE xscore IS NULL
            LIMIT batch_size
        )
        UPDATE validator
        SET xscore = 0
        WHERE ctid IN (SELECT ctid FROM cte);

        GET DIAGNOSTICS rows_updated = ROW_COUNT;  -- Get the number of rows updated in this batch

        -- Exit the loop if no more rows were updated
        IF rows_updated = 0 THEN
            EXIT;
        END IF;
    END LOOP;
END $$;

-- Step 3: Create the index on the 'xscore' column after updating
CREATE INDEX IF NOT EXISTS idx_validator_xscore ON validator (xscore);
