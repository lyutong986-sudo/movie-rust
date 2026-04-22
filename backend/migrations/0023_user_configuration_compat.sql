ALTER TABLE users
    ADD COLUMN IF NOT EXISTS configuration JSONB DEFAULT '{}'::jsonb;

UPDATE users
SET configuration = '{}'::jsonb
WHERE configuration IS NULL;

ALTER TABLE users
    ALTER COLUMN configuration SET DEFAULT '{}'::jsonb;

COMMENT ON COLUMN users.configuration IS '用户配置设置（JSON格式）';
