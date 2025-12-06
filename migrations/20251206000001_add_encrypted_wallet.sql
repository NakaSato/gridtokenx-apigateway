-- Add columns for encrypted wallet storage
ALTER TABLE users 
ADD COLUMN encrypted_private_key TEXT,
ADD COLUMN wallet_salt TEXT,
ADD COLUMN encryption_iv TEXT;

-- Create index for faster lookups if needed (though mostly accessed by ID)
-- CREATE INDEX idx_users_encrypted_key ON users(encrypted_private_key) WHERE encrypted_private_key IS NOT NULL;
