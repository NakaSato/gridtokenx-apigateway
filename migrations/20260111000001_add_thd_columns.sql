-- Migration: Add THD (Total Harmonic Distortion) and health_score columns to meter_readings table
-- These columns are referenced by the API but were missing from previous migrations

-- THD Parameters for power quality monitoring
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS thd_voltage DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS thd_current DOUBLE PRECISION NULL;

-- Health score for meter status tracking
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS health_score DOUBLE PRECISION NULL;

COMMENT ON COLUMN meter_readings.thd_voltage IS 'Total Harmonic Distortion for voltage (%)';
COMMENT ON COLUMN meter_readings.thd_current IS 'Total Harmonic Distortion for current (%)';
COMMENT ON COLUMN meter_readings.health_score IS 'Calculated health score of the meter reading (0-100)';
