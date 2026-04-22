ALTER TABLE users
ADD COLUMN IF NOT EXISTS configuration JSONB DEFAULT '{}'::jsonb;

COMMENT ON COLUMN users.configuration IS '用户配置设置（JSON格式）';
