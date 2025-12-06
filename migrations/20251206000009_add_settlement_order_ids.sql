-- Add order ID columns to settlements table for PDA lookup
ALTER TABLE settlements ADD COLUMN IF NOT EXISTS buy_order_id UUID;
ALTER TABLE settlements ADD COLUMN IF NOT EXISTS sell_order_id UUID;

-- Add foreign key constraints (optional but good practice)
-- ALTER TABLE settlements ADD CONSTRAINT fk_buy_order FOREIGN KEY (buy_order_id) REFERENCES trading_orders(id);
-- ALTER TABLE settlements ADD CONSTRAINT fk_sell_order FOREIGN KEY (sell_order_id) REFERENCES trading_orders(id);
