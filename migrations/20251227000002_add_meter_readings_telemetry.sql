-- Migration: Add telemetry columns to meter_readings table
-- This allows storage of full smart meter telemetry data

-- Electrical Parameters
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS voltage DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS current_amps DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS power_factor DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS frequency DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS temperature DOUBLE PRECISION NULL;

-- Location (GPS)
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS latitude DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION NULL;

-- Battery & Environmental
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS battery_level DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS weather_condition VARCHAR(50) NULL;

-- Trading & Certification
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS rec_eligible BOOLEAN DEFAULT false;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS carbon_offset DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS max_sell_price DOUBLE PRECISION NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS max_buy_price DOUBLE PRECISION NULL;

-- Security
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS meter_signature TEXT NULL;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS meter_type VARCHAR(50) NULL;

COMMENT ON COLUMN meter_readings.voltage IS 'Grid voltage in Volts';
COMMENT ON COLUMN meter_readings.current_amps IS 'Current in Amperes';
COMMENT ON COLUMN meter_readings.power_factor IS 'Power factor (0-1)';
COMMENT ON COLUMN meter_readings.frequency IS 'Grid frequency in Hz';
COMMENT ON COLUMN meter_readings.temperature IS 'Temperature in Celsius';
COMMENT ON COLUMN meter_readings.rec_eligible IS 'Renewable Energy Certificate eligible';
