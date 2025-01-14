-- Step 1: Add the new column 'epoch' to the 'vote' table if it doesn't exist
ALTER TABLE vote ADD COLUMN IF NOT EXISTS epoch NUMERIC;

-- Step 2: Update the 'epoch' column in batches to avoid locking the entire table
DO $$
DECLARE
    batch_size INT := 100000;  -- Define the batch size
    rows_updated INT;          -- Tracks the number of rows updated in each batch
BEGIN
    LOOP
        WITH cte AS (
            SELECT ctid
            FROM vote
            WHERE epoch IS NULL
            LIMIT batch_size
        )
        UPDATE vote
        SET epoch = 1
       WHERE ctid IN (SELECT ctid FROM cte);

        GET DIAGNOSTICS rows_updated = ROW_COUNT;  -- Get the number of rows updated in this batch

        -- Exit the loop if no more rows were updated
        IF rows_updated = 0 THEN
            EXIT;
        END IF;
    END LOOP;
END $$;

-- Step 3: Create the index on the 'epoch' column after all updates have been completed
CREATE INDEX IF NOT EXISTS idx_vote_epoch ON vote (epoch);