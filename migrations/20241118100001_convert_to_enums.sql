-- Convert VARCHAR columns to proper ENUM types
-- Created: November 18, 2024

-- Create order_side enum if it doesn't exist
DO $$ BEGIN
    CREATE TYPE order_side AS ENUM ('buy', 'sell');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Create order_status enum if it doesn't exist (lowercase to match existing code)
DO $$ BEGIN
    DROP TYPE IF EXISTS order_status CASCADE;
    CREATE TYPE order_status AS ENUM ('pending', 'active', 'partially_filled', 'filled', 'settled', 'cancelled', 'expired');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Drop existing default before converting
ALTER TABLE trading_orders ALTER COLUMN status DROP DEFAULT;

-- Convert trading_orders.status from VARCHAR to order_status enum
ALTER TABLE trading_orders 
    ALTER COLUMN status TYPE order_status 
    USING (
        CASE 
            WHEN LOWER(status) = 'pending' THEN 'pending'::order_status
            WHEN LOWER(status) = 'active' THEN 'active'::order_status
            WHEN LOWER(status) = 'partially_filled' THEN 'partially_filled'::order_status
            WHEN LOWER(status) = 'filled' THEN 'filled'::order_status
            WHEN LOWER(status) = 'settled' THEN 'settled'::order_status
            WHEN LOWER(status) = 'cancelled' THEN 'cancelled'::order_status
            WHEN LOWER(status) = 'expired' THEN 'expired'::order_status
            ELSE 'pending'::order_status
        END
    );

-- Convert trading_orders.side from VARCHAR to order_side enum
ALTER TABLE trading_orders 
    ALTER COLUMN side TYPE order_side 
    USING (
        CASE 
            WHEN LOWER(side) = 'buy' THEN 'buy'::order_side
            WHEN LOWER(side) = 'sell' THEN 'sell'::order_side
            ELSE 'buy'::order_side
        END
    );

-- Update default value for status
ALTER TABLE trading_orders ALTER COLUMN status SET DEFAULT 'pending'::order_status;

-- Drop old CHECK constraints
ALTER TABLE trading_orders DROP CONSTRAINT IF EXISTS chk_order_status;
ALTER TABLE trading_orders DROP CONSTRAINT IF EXISTS chk_order_type;
