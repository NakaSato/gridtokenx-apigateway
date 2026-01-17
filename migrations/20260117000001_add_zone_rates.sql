-- Zone-based pricing rates table
-- Allows configurable wheeling charges and loss factors per zone pair

CREATE TABLE IF NOT EXISTS zone_rates (
    id SERIAL PRIMARY KEY,
    from_zone_id INTEGER NOT NULL,
    to_zone_id INTEGER NOT NULL,
    wheeling_charge DECIMAL(10,4) NOT NULL,  -- THB per kWh
    loss_factor DECIMAL(5,4) NOT NULL,       -- Percentage (0.01 = 1%)
    description VARCHAR(255),
    effective_from TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    effective_until TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT zone_rates_unique_active UNIQUE NULLS NOT DISTINCT (from_zone_id, to_zone_id, effective_from)
);

-- Index for efficient lookups
CREATE INDEX idx_zone_rates_zones ON zone_rates(from_zone_id, to_zone_id) WHERE is_active = TRUE;
CREATE INDEX idx_zone_rates_active ON zone_rates(is_active, effective_from, effective_until);

-- Insert default zone rates based on current hardcoded values
INSERT INTO zone_rates (from_zone_id, to_zone_id, wheeling_charge, loss_factor, description) VALUES
    -- Same zone (intra-zone)
    (1, 1, 0.50, 0.01, 'Zone 1 local distribution'),
    (2, 2, 0.50, 0.01, 'Zone 2 local distribution'),
    (3, 3, 0.50, 0.01, 'Zone 3 local distribution'),
    (4, 4, 0.50, 0.01, 'Zone 4 local distribution'),
    (5, 5, 0.50, 0.01, 'Zone 5 local distribution'),
    -- Adjacent zones
    (1, 2, 1.00, 0.03, 'Zone 1 to 2 adjacent'),
    (2, 1, 1.00, 0.03, 'Zone 2 to 1 adjacent'),
    (2, 3, 1.00, 0.03, 'Zone 2 to 3 adjacent'),
    (3, 2, 1.00, 0.03, 'Zone 3 to 2 adjacent'),
    (3, 4, 1.00, 0.03, 'Zone 3 to 4 adjacent'),
    (4, 3, 1.00, 0.03, 'Zone 4 to 3 adjacent'),
    (4, 5, 1.00, 0.03, 'Zone 4 to 5 adjacent'),
    (5, 4, 1.00, 0.03, 'Zone 5 to 4 adjacent'),
    -- Cross-zone (distance 2)
    (1, 3, 1.70, 0.05, 'Zone 1 to 3 cross-zone'),
    (3, 1, 1.70, 0.05, 'Zone 3 to 1 cross-zone'),
    (2, 4, 1.70, 0.05, 'Zone 2 to 4 cross-zone'),
    (4, 2, 1.70, 0.05, 'Zone 4 to 2 cross-zone'),
    (3, 5, 1.70, 0.05, 'Zone 3 to 5 cross-zone'),
    (5, 3, 1.70, 0.05, 'Zone 5 to 3 cross-zone');

COMMENT ON TABLE zone_rates IS 'Configurable wheeling charges and loss factors per zone pair for P2P trading';
COMMENT ON COLUMN zone_rates.wheeling_charge IS 'Transmission fee in THB per kWh';
COMMENT ON COLUMN zone_rates.loss_factor IS 'Technical loss percentage (0.01 = 1%)';
