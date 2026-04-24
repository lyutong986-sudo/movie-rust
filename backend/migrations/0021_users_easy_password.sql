-- 追加 Emby "EasyPassword"（PIN）字段，配合 POST /Users/{Id}/EasyPassword
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS easy_password_hash text;
