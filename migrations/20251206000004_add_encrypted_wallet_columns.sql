-- Add encrypted wallet columns to users table (if not exists)
-- Note: These columns may already exist from migration 20251206000001
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name='users' AND column_name='encrypted_private_key') THEN
        ALTER TABLE users ADD COLUMN encrypted_private_key BYTEA;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name='users' AND column_name='wallet_salt') THEN
        ALTER TABLE users ADD COLUMN wallet_salt BYTEA;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns 
                   WHERE table_name='users' AND column_name='encryption_iv') THEN
        ALTER TABLE users ADD COLUMN encryption_iv BYTEA;
    END IF;
END $$;
