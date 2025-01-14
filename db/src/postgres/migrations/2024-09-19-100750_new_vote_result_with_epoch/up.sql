-- Step 1: Add the new column 'epoch' if it doesn't exist
ALTER TABLE vote_result ADD COLUMN IF NOT EXISTS epoch NUMERIC;

-- Step 2: Update the 'epoch' column in batches to avoid locking the entire table
DO $$
DECLARE
    batch_size INT := 100000; -- Batch size for updates
    updated_rows INT;
BEGIN
    LOOP
        -- Perform the batch update
        WITH cte AS (
            SELECT ctid
            FROM vote_result
            WHERE epoch IS NULL
            LIMIT batch_size
        )
        UPDATE vote_result
        SET epoch = 1
        WHERE ctid IN (SELECT ctid FROM cte);

        -- Get the number of rows updated
        GET DIAGNOSTICS updated_rows = ROW_COUNT;

        -- Exit the loop if no more rows were updated
        EXIT WHEN updated_rows = 0;
    END LOOP;
END $$;

-- Step 3: Create the index on the 'epoch' column after updating
CREATE INDEX IF NOT EXISTS idx_vote_result_epoch ON vote_result (epoch);
