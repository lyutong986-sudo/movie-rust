-- 补齐 Emby/Jellyfin 媒体流扩展字段。
-- 这些字段会在 PlaybackInfo、MediaSources 和网页播放器取流信息时读取。
ALTER TABLE media_streams
    ADD COLUMN IF NOT EXISTS attachment_size INTEGER,
    ADD COLUMN IF NOT EXISTS extended_video_sub_type TEXT,
    ADD COLUMN IF NOT EXISTS extended_video_sub_type_description TEXT,
    ADD COLUMN IF NOT EXISTS extended_video_type TEXT,
    ADD COLUMN IF NOT EXISTS is_anamorphic BOOLEAN,
    ADD COLUMN IF NOT EXISTS is_avc BOOLEAN,
    ADD COLUMN IF NOT EXISTS is_external_url TEXT,
    ADD COLUMN IF NOT EXISTS is_text_subtitle_stream BOOLEAN,
    ADD COLUMN IF NOT EXISTS level INTEGER,
    ADD COLUMN IF NOT EXISTS pixel_format TEXT,
    ADD COLUMN IF NOT EXISTS ref_frames INTEGER,
    ADD COLUMN IF NOT EXISTS stream_start_time_ticks BIGINT;

COMMENT ON COLUMN media_streams.attachment_size IS '附件流大小，例如内嵌字体附件大小';
COMMENT ON COLUMN media_streams.extended_video_sub_type IS '扩展视频子类型';
COMMENT ON COLUMN media_streams.extended_video_sub_type_description IS '扩展视频子类型描述';
COMMENT ON COLUMN media_streams.extended_video_type IS '扩展视频类型';
COMMENT ON COLUMN media_streams.is_anamorphic IS '是否为变形宽银幕视频';
COMMENT ON COLUMN media_streams.is_avc IS '是否为 AVC/H.264 视频';
COMMENT ON COLUMN media_streams.is_external_url IS '外部流 URL 标记';
COMMENT ON COLUMN media_streams.is_text_subtitle_stream IS '是否为文本字幕流';
COMMENT ON COLUMN media_streams.level IS '编码级别';
COMMENT ON COLUMN media_streams.pixel_format IS '像素格式';
COMMENT ON COLUMN media_streams.ref_frames IS '参考帧数量';
COMMENT ON COLUMN media_streams.stream_start_time_ticks IS '流起始时间，Emby ticks';
