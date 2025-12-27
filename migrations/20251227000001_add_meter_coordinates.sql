-- Migration: Add latitude and longitude coordinates to meters table
-- This allows meters to be displayed on the energy grid map

-- Add columns to meters table (primary table used by API)
ALTER TABLE meters ADD COLUMN IF NOT EXISTS latitude DOUBLE PRECISION NULL;
ALTER TABLE meters ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION NULL;

-- Also add to meter_registry for FK consistency
ALTER TABLE meter_registry ADD COLUMN IF NOT EXISTS latitude DOUBLE PRECISION NULL;
ALTER TABLE meter_registry ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION NULL;

-- Add index for geospatial queries on meters table
CREATE INDEX IF NOT EXISTS idx_meters_coordinates ON meters (latitude, longitude) WHERE latitude IS NOT NULL AND longitude IS NOT NULL;

COMMENT ON COLUMN meters.latitude IS 'Latitude coordinate for map display';
COMMENT ON COLUMN meters.longitude IS 'Longitude coordinate for map display';
