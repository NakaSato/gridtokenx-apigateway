-- Convert encrypted wallet columns from TEXT to BYTEA
-- This fixes the type mismatch where code expects BYTEA but columns are TEXT

ALTER TABLE users 
    ALTER COLUMN encrypted_private_key TYPE BYTEA USING encrypted_private_key::bytea,
    ALTER COLUMN wallet_salt TYPE BYTEA USING wallet_salt::bytea,
    ALTER COLUMN encryption_iv TYPE BYTEA USING encryption_iv::bytea;
