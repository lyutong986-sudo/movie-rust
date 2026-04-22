CREATE TABLE IF NOT EXISTS display_preferences (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    display_preferences_id text NOT NULL,
    client text NOT NULL DEFAULT 'emby',
    preferences jsonb NOT NULL DEFAULT '{}'::jsonb,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, display_preferences_id, client)
);

CREATE INDEX IF NOT EXISTS idx_display_preferences_user
    ON display_preferences(user_id, updated_at DESC);
