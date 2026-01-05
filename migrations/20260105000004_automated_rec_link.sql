-- Add meter_id to trading_orders and settlement_id to erc_certificates
-- Enable automated REC (Energy Attribute Certificate) issuance linked to settlements

-- 1. Add meter_id to trading_orders
ALTER TABLE trading_orders
ADD COLUMN IF NOT EXISTS meter_id UUID REFERENCES meters (id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_trading_orders_meter ON trading_orders (meter_id);

-- 2. Add settlement_id to erc_certificates
ALTER TABLE erc_certificates
ADD COLUMN IF NOT EXISTS settlement_id UUID REFERENCES settlements (id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_erc_certificates_settlement ON erc_certificates (settlement_id);

-- 3. Add comment for clarity
COMMENT ON COLUMN trading_orders.meter_id IS 'The specific meter that generated the energy (for sell orders)';

COMMENT ON COLUMN erc_certificates.settlement_id IS 'Reference to the settlement that triggered this certificate issuance';