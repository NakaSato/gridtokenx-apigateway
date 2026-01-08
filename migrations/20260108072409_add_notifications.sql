-- Add notifications system for real-time push alerts
-- Migration: 20260108072409_add_notifications

-- Create notification type enum
DO $$ BEGIN
    CREATE TYPE notification_type AS ENUM (
        'order_filled',
        'order_matched',
        'conditional_triggered',
        'recurring_executed',
        'price_alert',
        'escrow_released',
        'system'
    );
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- User notification preferences
CREATE TABLE IF NOT EXISTS user_notification_preferences (
    user_id UUID PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    order_filled BOOLEAN DEFAULT true,
    order_matched BOOLEAN DEFAULT true,
    conditional_triggered BOOLEAN DEFAULT true,
    recurring_executed BOOLEAN DEFAULT true,
    price_alerts BOOLEAN DEFAULT true,
    escrow_events BOOLEAN DEFAULT true,
    system_announcements BOOLEAN DEFAULT true,
    email_enabled BOOLEAN DEFAULT false,
    push_enabled BOOLEAN DEFAULT true,
    updated_at TIMESTAMPTZ DEFAULT NOW ()
);

-- Notifications table
CREATE TABLE IF NOT EXISTS notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    notification_type notification_type NOT NULL,
    title VARCHAR(200) NOT NULL,
    message TEXT,
    data JSONB,
    read BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW ()
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_notifications_user_unread ON notifications (user_id, created_at DESC)
WHERE
    read = false;

CREATE INDEX IF NOT EXISTS idx_notifications_user_all ON notifications (user_id, created_at DESC);

-- Comments
COMMENT ON TABLE user_notification_preferences IS 'User preferences for notification types and delivery methods';

COMMENT ON TABLE notifications IS 'Notification history for users';

COMMENT ON COLUMN notifications.data IS 'JSON data with additional context (order_id, amount, price, etc.)';