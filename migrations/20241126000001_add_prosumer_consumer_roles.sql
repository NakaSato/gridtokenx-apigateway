-- Add prosumer and consumer roles back to user role constraint
-- This fixes the mismatch between API code (which checks for 'prosumer')
-- and database constraint (which only allowed 'user' and 'admin')
-- Created: November 26, 2025

-- Step 1: Drop existing constraint
ALTER TABLE users DROP CONSTRAINT IF EXISTS chk_user_role;

-- Step 2: Add updated constraint with all required roles
ALTER TABLE users ADD CONSTRAINT chk_user_role 
    CHECK (role IN ('user', 'admin', 'prosumer', 'consumer'));

-- Step 3: Update index to include new roles
DROP INDEX IF EXISTS idx_users_role_simplified;
CREATE INDEX idx_users_role ON users(role) 
    WHERE role IN ('user', 'admin', 'prosumer', 'consumer');

-- Step 4: Add comment explaining role types
COMMENT ON COLUMN users.role IS 'User role: user (default), admin (full access), prosumer (can submit meter readings), consumer (can purchase energy)';
