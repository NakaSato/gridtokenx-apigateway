-- Create wallet audit log table for security monitoring
CREATE TABLE wallet_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    operation TEXT NOT NULL,
    success BOOLEAN NOT NULL DEFAULT true,
    ip_address INET,
    user_agent TEXT,
    error_message TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for querying by user
CREATE INDEX idx_wallet_audit_log_user_id ON wallet_audit_log(user_id);

-- Index for querying by operation type
CREATE INDEX idx_wallet_audit_log_operation ON wallet_audit_log(operation);

-- Index for querying by timestamp
CREATE INDEX idx_wallet_audit_log_created_at ON wallet_audit_log(created_at DESC);

-- Index for failed operations
CREATE INDEX idx_wallet_audit_log_failures ON wallet_audit_log(success) WHERE success = false;
