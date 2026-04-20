-- 首先，创建更新时间戳函数（如果不存在）
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- 人物表，存储演员、导演、编剧等信息
CREATE TABLE IF NOT EXISTS persons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    sort_name TEXT,
    overview TEXT,
    external_url TEXT,
    provider_ids JSONB DEFAULT '{}'::jsonb,
    -- 元数据
    premiere_date TIMESTAMP WITH TIME ZONE,
    production_year INTEGER,
    -- 图片路径
    primary_image_path TEXT,
    backdrop_image_path TEXT,
    logo_image_path TEXT,
    -- 统计信息
    favorite_count INTEGER DEFAULT 0,
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    -- 索引
    UNIQUE(name, sort_name)
);

-- 人物角色关联表，关联人物和媒体项，并指定角色类型
CREATE TABLE IF NOT EXISTS person_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id UUID NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    role_type TEXT NOT NULL CHECK (role_type IN ('Actor', 'Director', 'Writer', 'Producer', 'Composer', 'Cinematographer', 'Editor', 'Other')),
    role TEXT,  -- 具体角色名称（如角色名）
    sort_order INTEGER DEFAULT 0,
    -- 元数据
    is_featured BOOLEAN DEFAULT false,
    is_leading_role BOOLEAN DEFAULT false,
    is_recurring BOOLEAN DEFAULT false,
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT now(),
    -- 索引
    UNIQUE(person_id, media_item_id, role_type, role),
    CONSTRAINT check_role_not_empty CHECK (role IS NULL OR trim(role) != '')
);

-- 创建索引以加速查询（如果不存在）
CREATE INDEX IF NOT EXISTS idx_persons_name ON persons(name);
CREATE INDEX IF NOT EXISTS idx_persons_sort_name ON persons(sort_name);
CREATE INDEX IF NOT EXISTS idx_persons_production_year ON persons(production_year);
CREATE INDEX IF NOT EXISTS idx_person_roles_person_id ON person_roles(person_id);
CREATE INDEX IF NOT EXISTS idx_person_roles_media_item_id ON person_roles(media_item_id);
CREATE INDEX IF NOT EXISTS idx_person_roles_role_type ON person_roles(role_type);

-- 更新时间戳触发器（先删除再创建以确保幂等性）
DROP TRIGGER IF EXISTS update_persons_updated_at ON persons;
CREATE TRIGGER update_persons_updated_at
    BEFORE UPDATE ON persons
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

DROP TRIGGER IF EXISTS update_person_roles_updated_at ON person_roles;
CREATE TRIGGER update_person_roles_updated_at
    BEFORE UPDATE ON person_roles
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- 注释
COMMENT ON TABLE persons IS '人物表，存储演员、导演、编剧等信息';
COMMENT ON COLUMN persons.provider_ids IS '外部提供者ID（如TMDB、IMDb等）的JSON映射';
COMMENT ON COLUMN persons.premiere_date IS '首次亮相日期（如演员出道时间）';
COMMENT ON COLUMN persons.production_year IS '生产年份（如导演首部作品年份）';

COMMENT ON TABLE person_roles IS '人物角色关联表，关联人物和媒体项';
COMMENT ON COLUMN person_roles.role_type IS '角色类型：Actor, Director, Writer, Producer, Composer, Cinematographer, Editor, Other';
COMMENT ON COLUMN person_roles.role IS '具体角色名称（如电影中的角色名）';
COMMENT ON COLUMN person_roles.sort_order IS '排序顺序（用于同一媒体项中同一类型角色的排序）';
COMMENT ON COLUMN person_roles.is_featured IS '是否为特色角色（如主演）';
COMMENT ON COLUMN person_roles.is_leading_role IS '是否为主演';
COMMENT ON COLUMN person_roles.is_recurring IS '是否为常驻角色（用于剧集）';