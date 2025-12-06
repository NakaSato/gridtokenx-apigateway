-- Migration: Add encryption key version tracking
-- This enables key rotation with backward compatibility

-- Track encryption key versions
CREATE TABLE encryption_keys (
    version INTEGER PRIMARY KEY,
    key_hash VARCHAR(64) NOT NULL,  -- SHA-256 hash of the key for verification
    created_at TIMESTAMPTZ DEFAULT NOW(),
    rotated_at TIMESTAMPTZ,
    is_active BOOLEAN DEFAULT true,
    notes TEXT
);

-- Add key version to users table
ALTER TABLE users ADD COLUMN key_version INTEGER DEFAULT 1;

-- Create indexes for efficient lookups
CREATE INDEX idx_encryption_keys_active ON encryption_keys(is_active) WHERE is_active = true;
CREATE INDEX idx_users_key_version ON users(key_version);

-- Insert initial key version (current encryption key)
INSERT INTO encryption_keys (version, key_hash, notes) 
VALUES (1, 'initial_key_placeholder', 'Initial encryption key - hash to be updated on first rotation');

-- Add foreign key constraint (after inserting initial version)
ALTER TABLE users ADD CONSTRAINT fk_users_key_version 
    FOREIGN KEY (key_version) REFERENCES encryption_keys(version);

-- Comments for documentation
COMMENT ON TABLE encryption_keys IS 'Tracks encryption key versions for key rotation';
COMMENT ON COLUMN encryption_keys.key_hash IS 'SHA-256 hash of the encryption key for verification';
COMMENT ON COLUMN encryption_keys.is_active IS 'Only one key should be active at a time';
COMMENT ON COLUMN users.key_version IS 'References the encryption key version used for this user wallet';
