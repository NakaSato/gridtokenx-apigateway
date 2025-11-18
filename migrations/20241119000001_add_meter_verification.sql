-- Migration: Add meter verification tables
-- Date: 2025-11-19
-- Purpose: Add meter registry and verification attempts tables for security

-- Create meter_registry table
CREATE TABLE IF NOT EXISTS meter_registry (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    meter_serial VARCHAR(100) UNIQUE NOT NULL,
    meter_key_hash VARCHAR(255) NOT NULL,  -- bcrypt hash of meter key
    verification_method VARCHAR(50) NOT NULL DEFAULT 'serial',  -- serial, api_key, qr_code, challenge
    verification_status VARCHAR(50) NOT NULL DEFAULT 'pending',  -- pending, verified, rejected, suspended
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Meter metadata
    manufacturer VARCHAR(100),
    meter_type VARCHAR(50) NOT NULL,  -- residential, commercial, solar, industrial
    location_address TEXT,
    installation_date DATE,
    
    -- Verification details
    verification_proof TEXT,  -- Utility bill reference or other proof
    verified_at TIMESTAMP WITH TIME ZONE,
    verified_by UUID REFERENCES users(id),  -- Admin who verified
    
    -- Timestamps
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT meter_registry_check_status CHECK (verification_status IN ('pending', 'verified', 'rejected', 'suspended')),
    CONSTRAINT meter_registry_check_method CHECK (verification_method IN ('serial', 'api_key', 'qr_code', 'challenge')),
    CONSTRAINT meter_registry_check_type CHECK (meter_type IN ('residential', 'commercial', 'solar', 'industrial'))
);

-- Create meter_verification_attempts table for audit trail
CREATE TABLE IF NOT EXISTS meter_verification_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    meter_serial VARCHAR(100) NOT NULL,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    verification_method VARCHAR(50) NOT NULL,
    
    -- Attempt details
    ip_address INET,
    user_agent TEXT,
    attempt_result VARCHAR(50) NOT NULL,  -- success, invalid_key, meter_claimed, rate_limited, system_error
    failure_reason TEXT,
    
    -- Timing
    attempted_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    
    -- Constraints
    CONSTRAINT meter_attempts_check_result CHECK (attempt_result IN ('success', 'invalid_key', 'meter_claimed', 'rate_limited', 'system_error'))
);

-- Update meter_readings table to reference meter_registry
ALTER TABLE meter_readings 
ADD COLUMN IF NOT EXISTS meter_id UUID REFERENCES meter_registry(id),
ADD COLUMN IF NOT EXISTS verification_status VARCHAR(50) DEFAULT 'legacy_unverified';

-- Add constraint for verification_status in meter_readings
ALTER TABLE meter_readings 
ADD CONSTRAINT meter_readings_check_verification_status 
CHECK (verification_status IN ('verified', 'legacy_unverified', 'pending'));

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_meter_registry_user_id ON meter_registry(user_id);
CREATE INDEX IF NOT EXISTS idx_meter_registry_meter_serial ON meter_registry(meter_serial);
CREATE INDEX IF NOT EXISTS idx_meter_registry_status ON meter_registry(verification_status);
CREATE INDEX IF NOT EXISTS idx_meter_attempts_user_id ON meter_verification_attempts(user_id);
CREATE INDEX IF NOT EXISTS idx_meter_attempts_serial ON meter_verification_attempts(meter_serial);
CREATE INDEX IF NOT EXISTS idx_meter_attempts_attempted_at ON meter_verification_attempts(attempted_at);
CREATE INDEX IF NOT EXISTS idx_meter_readings_meter_id ON meter_readings(meter_id);
CREATE INDEX IF NOT EXISTS idx_meter_readings_verification_status ON meter_readings(verification_status);

-- Create updated_at trigger
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_meter_registry_updated_at 
    BEFORE UPDATE ON meter_registry 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Insert sample data for testing (optional)
-- This would be removed in production
INSERT INTO meter_registry (meter_serial, meter_key_hash, verification_method, verification_status, user_id, meter_type)
SELECT 
    'TEST-METER-001',
    '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewdBPj6ukx.LFvO', -- bcrypt hash of 'test-key-12345'
    'serial',
    'verified',
    id,
    'residential'
FROM users 
WHERE email = 'test@example.com'
LIMIT 1
ON CONFLICT (meter_serial) DO NOTHING;

COMMENT ON TABLE meter_registry IS 'Registry of verified smart meters with ownership proof';
COMMENT ON TABLE meter_verification_attempts IS 'Audit log of all meter verification attempts';
COMMENT ON COLUMN meter_registry.meter_key_hash IS 'bcrypt hash of the meter key (never store plaintext)';
COMMENT ON COLUMN meter_readings.verification_status IS 'Tracks if reading is from verified meter or legacy unverified';
