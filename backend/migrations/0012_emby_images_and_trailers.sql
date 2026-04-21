-- 补齐 Emby 详情页常用图片类型和远程预告片字段。
ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS logo_path text,
    ADD COLUMN IF NOT EXISTS thumb_path text,
    ADD COLUMN IF NOT EXISTS remote_trailers text[] NOT NULL DEFAULT ARRAY[]::text[];

COMMENT ON COLUMN media_items.logo_path IS 'Logo 图片路径';
COMMENT ON COLUMN media_items.thumb_path IS 'Thumb 图片路径';
COMMENT ON COLUMN media_items.remote_trailers IS '远程预告片 URL 列表';
