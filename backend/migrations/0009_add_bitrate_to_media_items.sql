-- 添加比特率字段到媒体项表
ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS bit_rate BIGINT;

-- 注释
COMMENT ON COLUMN media_items.bit_rate IS '媒体文件总比特率（bps）';