-- 为用户表添加策略字段
ALTER TABLE users ADD COLUMN IF NOT EXISTS policy JSONB DEFAULT '{}'::jsonb;

-- 注释
COMMENT ON COLUMN users.policy IS '用户策略设置（JSON格式）';