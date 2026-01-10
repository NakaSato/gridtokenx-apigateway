-- Add session_token to settlements
-- This allows automated settlement to use session-cached keys from either party if required

ALTER TABLE settlements
ADD COLUMN IF NOT EXISTS seller_session_token VARCHAR(128),
ADD COLUMN IF NOT EXISTS buyer_session_token VARCHAR(128);

COMMENT ON COLUMN settlements.seller_session_token IS 'Session token used for password-less signing of energy transfers';

COMMENT ON COLUMN settlements.buyer_session_token IS 'Session token used for password-less signing of payments (if applicable)';