-- Add zone_id to meter tables
-- Created: 2026-01-05

-- 1. Add zone_id to meter_registry (the main table used by API)
ALTER TABLE meter_registry ADD COLUMN IF NOT EXISTS zone_id INTEGER;

-- 2. Add zone_id to meters (the table sometimes used by sync/simulator)
ALTER TABLE meters ADD COLUMN IF NOT EXISTS zone_id INTEGER;

-- 3. Add index for performance in matching engine lookups
CREATE INDEX IF NOT EXISTS idx_meter_registry_zone_id ON meter_registry (zone_id);