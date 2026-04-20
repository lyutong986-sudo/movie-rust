-- 为sessions表添加expires_at字段，支持令牌过期

ALTER TABLE sessions
    ADD COLUMN expires_at TIMESTAMP WITH TIME ZONE DEFAULT NULL;

COMMENT ON COLUMN sessions.expires_at IS '会话过期时间，NULL表示永不过期';

-- 创建索引以加速过期会话清理
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);

-- 更新触发器以包含expires_at字段（如果需要）
-- 注意：update_updated_at_column函数已在之前的迁移中定义
-- 这里不需要修改触发器，因为它只更新updated_at字段