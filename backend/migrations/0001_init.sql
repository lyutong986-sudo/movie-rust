CREATE TABLE IF NOT EXISTS users (
    id uuid PRIMARY KEY,
    name text NOT NULL UNIQUE,
    password_hash text NOT NULL,
    is_admin boolean NOT NULL DEFAULT false,
    is_hidden boolean NOT NULL DEFAULT false,
    is_disabled boolean NOT NULL DEFAULT false,
    policy jsonb NOT NULL DEFAULT '{}'::jsonb,
    configuration jsonb NOT NULL DEFAULT '{}'::jsonb,
    primary_image_path text,
    backdrop_image_path text,
    logo_image_path text,
    date_modified timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sessions (
    access_token text PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id text,
    device_name text,
    client text,
    application_version text,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_activity_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS libraries (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    collection_type text NOT NULL DEFAULT 'movies',
    path text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS media_items (
    id uuid PRIMARY KEY,
    library_id uuid NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    parent_id uuid REFERENCES media_items(id) ON DELETE CASCADE,
    name text NOT NULL,
    sort_name text NOT NULL,
    item_type text NOT NULL DEFAULT 'Movie',
    media_type text NOT NULL DEFAULT 'Video',
    path text NOT NULL,
    container text,
    overview text,
    production_year integer,
    runtime_ticks bigint,
    premiere_date date,
    image_primary_path text,
    backdrop_path text,
    date_created timestamptz NOT NULL DEFAULT now(),
    date_modified timestamptz NOT NULL DEFAULT now(),
    UNIQUE (library_id, path)
);

CREATE TABLE IF NOT EXISTS user_item_data (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    playback_position_ticks bigint NOT NULL DEFAULT 0,
    play_count integer NOT NULL DEFAULT 0,
    is_favorite boolean NOT NULL DEFAULT false,
    is_played boolean NOT NULL DEFAULT false,
    last_played_date timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, item_id)
);

CREATE TABLE IF NOT EXISTS playback_events (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id uuid REFERENCES media_items(id) ON DELETE SET NULL,
    session_id text,
    event_type text NOT NULL,
    position_ticks bigint,
    is_paused boolean,
    played_to_completion boolean,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_media_items_library ON media_items(library_id);
CREATE INDEX IF NOT EXISTS idx_media_items_parent ON media_items(parent_id);
CREATE INDEX IF NOT EXISTS idx_media_items_type ON media_items(item_type);
CREATE INDEX IF NOT EXISTS idx_media_items_sort ON media_items(sort_name);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
