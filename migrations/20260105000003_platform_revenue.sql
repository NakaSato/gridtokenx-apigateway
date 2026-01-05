-- Migration: Platform Revenue Tracking
-- Adds support for tracking fees, wheeling charges, and grid losses

CREATE TABLE IF NOT EXISTS platform_revenue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    settlement_id UUID NOT NULL REFERENCES settlements (id) ON DELETE CASCADE,
    amount NUMERIC(20, 8) NOT NULL,
    revenue_type VARCHAR(20) NOT NULL, -- 'platform_fee', 'wheeling_charge', 'loss_cost'
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW (),
    CONSTRAINT chk_revenue_type CHECK (
        revenue_type IN (
            'platform_fee',
            'wheeling_charge',
            'loss_cost'
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_platform_revenue_settlement ON platform_revenue (settlement_id);

CREATE INDEX IF NOT EXISTS idx_platform_revenue_type ON platform_revenue (revenue_type);

CREATE INDEX IF NOT EXISTS idx_platform_revenue_date ON platform_revenue (created_at);