-- Add recurring orders table for DCA-style scheduled trading
-- Migration: 20260108071706_add_recurring_orders

-- Create interval type enum
DO $$ BEGIN
    CREATE TYPE interval_type AS ENUM ('hourly', 'daily', 'weekly', 'monthly');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Create recurring order status enum
DO $$ BEGIN
    CREATE TYPE recurring_status AS ENUM ('active', 'paused', 'completed', 'cancelled');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Create recurring_orders table
CREATE TABLE IF NOT EXISTS recurring_orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

-- Order details
side order_side NOT NULL,
energy_amount DECIMAL(20, 8) NOT NULL,
max_price_per_kwh DECIMAL(20, 8), -- Max price for buy orders
min_price_per_kwh DECIMAL(20, 8), -- Min price for sell orders

-- Schedule configuration
interval_type interval_type NOT NULL,
interval_value INT DEFAULT 1 CHECK (interval_value > 0),
next_execution_at TIMESTAMPTZ NOT NULL,
last_executed_at TIMESTAMPTZ,

-- Status and limits
status recurring_status DEFAULT 'active',
total_executions INT DEFAULT 0,
max_executions INT, -- NULL = unlimited

-- Metadata
name VARCHAR(100), -- User-friendly name
description TEXT,

-- Timestamps
created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Index for scheduler to find orders ready to execute
CREATE INDEX IF NOT EXISTS idx_recurring_orders_next_exec ON recurring_orders (next_execution_at)
WHERE
    status = 'active';

-- Index for user's orders lookup
CREATE INDEX IF NOT EXISTS idx_recurring_orders_user ON recurring_orders (user_id, status);

-- Table to track execution history

CREATE TABLE IF NOT EXISTS recurring_order_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    recurring_order_id UUID NOT NULL REFERENCES recurring_orders(id) ON DELETE CASCADE,
    trading_order_id UUID REFERENCES trading_orders(id),
    
    executed_at TIMESTAMPTZ DEFAULT NOW(),
    status VARCHAR(20) NOT NULL,            -- 'success', 'failed', 'skipped'
    error_message TEXT,

-- Execution details
energy_amount DECIMAL(20,8),
    price_per_kwh DECIMAL(20,8),
    
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_recurring_executions_order ON recurring_order_executions (
    recurring_order_id,
    executed_at DESC
);

-- Comments
COMMENT ON TABLE recurring_orders IS 'DCA-style recurring orders that execute at scheduled intervals';

COMMENT ON COLUMN recurring_orders.interval_type IS 'How often to execute: hourly, daily, weekly, monthly';

COMMENT ON COLUMN recurring_orders.interval_value IS 'Execute every N intervals (e.g., every 2 days)';

COMMENT ON COLUMN recurring_orders.max_executions IS 'Maximum number of executions (NULL = unlimited)';