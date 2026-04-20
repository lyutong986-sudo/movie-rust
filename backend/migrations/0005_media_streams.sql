-- 首先，创建更新时间戳函数（如果不存在）
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- 媒体流表，存储视频、音频、字幕轨道信息
CREATE TABLE IF NOT EXISTS media_streams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    index INTEGER NOT NULL,  -- 流索引
    stream_type TEXT NOT NULL CHECK (stream_type IN ('video', 'audio', 'subtitle', 'data', 'attachment')),
    codec TEXT,
    codec_tag TEXT,
    language TEXT,
    title TEXT,
    is_default BOOLEAN DEFAULT false,
    is_forced BOOLEAN DEFAULT false,
    is_external BOOLEAN DEFAULT false,
    is_hearing_impaired BOOLEAN DEFAULT false,
    profile TEXT,
    width INTEGER,
    height INTEGER,
    channels INTEGER,
    sample_rate INTEGER,
    bit_rate INTEGER,
    bit_depth INTEGER,
    channel_layout TEXT,
    aspect_ratio TEXT,
    average_frame_rate FLOAT,
    real_frame_rate FLOAT,
    is_interlaced BOOLEAN DEFAULT false,
    color_range TEXT,
    color_space TEXT,
    color_transfer TEXT,
    color_primaries TEXT,
    rotation INTEGER,
    hdr10_plus_present_flag BOOLEAN,
    dv_version_major INTEGER,
    dv_version_minor INTEGER,
    dv_profile INTEGER,
    dv_level INTEGER,
    dv_bl_signal_compatibility_id INTEGER,
    comment TEXT,
    time_base TEXT,
    codec_time_base TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    -- 确保同一个媒体项的流索引和类型组合唯一
    UNIQUE(media_item_id, index, stream_type)
);

-- 创建索引以加速查询（如果不存在）
CREATE INDEX IF NOT EXISTS idx_media_streams_media_item_id ON media_streams(media_item_id);
CREATE INDEX IF NOT EXISTS idx_media_streams_stream_type ON media_streams(stream_type);
CREATE INDEX IF NOT EXISTS idx_media_streams_language ON media_streams(language);
CREATE INDEX IF NOT EXISTS idx_media_streams_codec ON media_streams(codec);

-- 更新时间戳触发器（先删除再创建以确保幂等性）
DROP TRIGGER IF EXISTS update_media_streams_updated_at ON media_streams;
CREATE TRIGGER update_media_streams_updated_at
    BEFORE UPDATE ON media_streams
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- 注释
COMMENT ON TABLE media_streams IS '媒体流表，存储视频、音频、字幕轨道信息';
COMMENT ON COLUMN media_streams.index IS '流在媒体文件中的索引（从0开始）';
COMMENT ON COLUMN media_streams.stream_type IS '流类型：video, audio, subtitle, data, attachment';
COMMENT ON COLUMN media_streams.is_default IS '是否为默认轨道';
COMMENT ON COLUMN media_streams.is_forced IS '是否为强制轨道（如强制字幕）';
COMMENT ON COLUMN media_streams.is_external IS '是否为外部流（如外挂字幕）';
COMMENT ON COLUMN media_streams.is_hearing_impaired IS '是否为听力障碍者字幕';
COMMENT ON COLUMN media_streams.profile IS '编码配置文件（如High, Main, Baseline）';
COMMENT ON COLUMN media_streams.dv_profile IS 'Dolby Vision配置文件';
COMMENT ON COLUMN media_streams.dv_level IS 'Dolby Vision级别';