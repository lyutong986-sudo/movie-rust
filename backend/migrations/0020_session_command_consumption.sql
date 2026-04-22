ALTER TABLE session_commands
    ADD COLUMN IF NOT EXISTS consumed_at timestamptz;

CREATE INDEX IF NOT EXISTS idx_session_commands_unconsumed
    ON session_commands(session_id, created_at)
    WHERE consumed_at IS NULL;
