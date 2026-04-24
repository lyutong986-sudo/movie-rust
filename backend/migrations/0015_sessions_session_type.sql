ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS session_type text NOT NULL DEFAULT 'Interactive';

UPDATE sessions
SET session_type = 'Interactive'
WHERE session_type IS NULL OR btrim(session_type) = '';

CREATE INDEX IF NOT EXISTS idx_sessions_session_type ON sessions(session_type);
