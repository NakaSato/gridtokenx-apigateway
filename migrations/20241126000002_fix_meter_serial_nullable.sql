-- Fix meter_serial NOT NULL constraint to allow readings without meter serial
-- This supports legacy/unverified meters that don't have a serial number
-- Created: November 26, 2025

-- Make meter_serial nullable
ALTER TABLE meter_readings ALTER COLUMN meter_serial DROP NOT NULL;

-- Add comment explaining the change
COMMENT ON COLUMN meter_readings.meter_serial IS 'Meter serial number (optional for legacy/unverified meters)';
