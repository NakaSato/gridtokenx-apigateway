-- Migration: Add wallet export rate limiting
-- This table tracks when users last exported their wallet to enforce rate limits

CREATE TABLE wallet_export_rate_limit (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    last_export_at TIMESTAMPTZ NOT NULL,
    export_count INTEGER DEFAULT 1,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for efficient queries
CREATE INDEX idx_wallet_export_rate_limit_user_id ON wallet_export_rate_limit(user_id);
CREATE INDEX idx_wallet_export_rate_limit_last_export ON wallet_export_rate_limit(last_export_at);

-- Comments for documentation
COMMENT ON TABLE wallet_export_rate_limit IS 'Tracks wallet export attempts for rate limiting (1 export per hour)';
COMMENT ON COLUMN wallet_export_rate_limit.last_export_at IS 'Timestamp of the last successful export';
COMMENT ON COLUMN wallet_export_rate_limit.export_count IS 'Total number of exports by this user';
