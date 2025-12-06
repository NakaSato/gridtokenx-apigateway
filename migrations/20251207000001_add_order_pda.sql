-- Add order_pda column to trading_orders table
-- This stores the on-chain Order PDA address for settlement execution

ALTER TABLE trading_orders
ADD COLUMN order_pda TEXT;

-- Add index for faster lookups
CREATE INDEX idx_trading_orders_order_pda ON trading_orders(order_pda);
