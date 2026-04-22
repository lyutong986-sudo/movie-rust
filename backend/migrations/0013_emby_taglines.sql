-- 补齐 Emby BaseItemDto 标准 Taglines 字段，供详情页与本地播放器使用。
ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS taglines text[] NOT NULL DEFAULT ARRAY[]::text[];

COMMENT ON COLUMN media_items.taglines IS 'Emby BaseItemDto.Taglines 文案列表';
