-- Migration: Add zone data snapshots to grid_status_history
-- Created: 2026-01-10

ALTER TABLE grid_status_history
ADD COLUMN IF NOT EXISTS zones_data JSONB;

COMMENT ON COLUMN grid_status_history.zones_data IS 'JSONB snapshot of per-zone grid metrics {zone_id: {generation, consumption, net_balance, active_meters}}';