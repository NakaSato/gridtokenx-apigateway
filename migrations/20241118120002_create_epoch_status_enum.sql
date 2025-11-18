-- Create epoch_status enum type
-- Created: November 18, 2024

-- Create epoch_status enum if it doesn't exist
DO $$ BEGIN
    CREATE TYPE epoch_status AS ENUM ('pending', 'active', 'cleared', 'settled');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Drop existing default before converting
ALTER TABLE market_epochs ALTER COLUMN status DROP DEFAULT;

-- Convert market_epochs.status from VARCHAR to epoch_status enum
ALTER TABLE market_epochs 
    ALTER COLUMN status TYPE epoch_status 
    USING (
        CASE 
            WHEN LOWER(status) = 'pending' THEN 'pending'::epoch_status
            WHEN LOWER(status) = 'active' THEN 'active'::epoch_status
            WHEN LOWER(status) = 'cleared' THEN 'cleared'::epoch_status
            WHEN LOWER(status) = 'settled' THEN 'settled'::epoch_status
            ELSE 'pending'::epoch_status
        END
    );

-- Update default value for status
ALTER TABLE market_epochs ALTER COLUMN status SET DEFAULT 'pending'::epoch_status;

-- Drop old CHECK constraint since enum provides the constraint
ALTER TABLE market_epochs DROP CONSTRAINT IF EXISTS chk_epoch_status;
