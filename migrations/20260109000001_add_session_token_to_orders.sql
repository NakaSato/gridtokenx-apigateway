-- Add session_token to trading_orders and recurring_orders
-- This allows automated execution (conditional/recurring) to use session-cached keys

ALTER TABLE trading_orders
ADD COLUMN IF NOT EXISTS session_token VARCHAR(128);

ALTER TABLE recurring_orders
ADD COLUMN IF NOT EXISTS session_token VARCHAR(128);

-- Create index for session-based order retrieval
CREATE INDEX IF NOT EXISTS idx_trading_orders_session ON trading_orders (session_token)
WHERE
    session_token IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_recurring_orders_session ON recurring_orders (session_token)
WHERE
    session_token IS NOT NULL;

COMMENT ON COLUMN trading_orders.session_token IS 'Session token used for password-less signing when order is triggered';

COMMENT ON COLUMN recurring_orders.session_token IS 'Session token used for password-less signing of recurring executions';