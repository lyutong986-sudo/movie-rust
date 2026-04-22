CREATE TABLE IF NOT EXISTS session_play_queue (
    session_id text NOT NULL REFERENCES sessions(access_token) ON DELETE CASCADE,
    item_id uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    playlist_item_id text NOT NULL DEFAULT md5(random()::text || clock_timestamp()::text),
    sort_index integer NOT NULL DEFAULT 0,
    position_ticks bigint,
    is_paused boolean,
    play_state text NOT NULL DEFAULT 'Playing',
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, item_id)
);

CREATE INDEX IF NOT EXISTS idx_session_play_queue_session
    ON session_play_queue(session_id, sort_index);

CREATE TABLE IF NOT EXISTS session_commands (
    id uuid PRIMARY KEY,
    session_id text NOT NULL REFERENCES sessions(access_token) ON DELETE CASCADE,
    command text NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_session_commands_session
    ON session_commands(session_id, created_at DESC);
