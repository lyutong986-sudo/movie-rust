CREATE TABLE IF NOT EXISTS system_settings (
    key text PRIMARY KEY,
    value jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO system_settings (key, value)
SELECT 'startup_wizard_completed', to_jsonb(EXISTS (SELECT 1 FROM users))
ON CONFLICT (key) DO NOTHING;
