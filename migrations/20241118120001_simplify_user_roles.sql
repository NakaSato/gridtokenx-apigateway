-- Simplify user roles from 5 to 2
-- Remove: ami, producer, consumer
-- Keep: user, admin
-- Created: November 18, 2025

-- Step 1: Update existing users to new roles
-- Convert producer, consumer, and ami to 'user'
UPDATE users 
SET role = 'user' 
WHERE role IN ('producer', 'consumer', 'ami');

-- Step 2: Add check constraint to enforce only 'user' and 'admin'
ALTER TABLE users DROP CONSTRAINT IF EXISTS chk_user_role;
ALTER TABLE users ADD CONSTRAINT chk_user_role 
    CHECK (role IN ('user', 'admin'));

-- Step 3: Update default value
ALTER TABLE users ALTER COLUMN role SET DEFAULT 'user';

-- Step 4: Create index on role column (if not exists)
CREATE INDEX IF NOT EXISTS idx_users_role_simplified ON users(role) WHERE role IN ('user', 'admin');
