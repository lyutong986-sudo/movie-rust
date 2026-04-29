use crate::error::AppError;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use sha1::{Digest, Sha1};

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AppError::Internal(format!("密码哈希失败: {error}")))
}

pub fn verify_password(password_hash: &str, password: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

/// 校验来自外部用户库（Emby SQLite `LocalUsersv2`）的旧版密码格式。
///
/// 当前支持的 `format`：
/// - `"emby_sha1"`：`hex(SHA1(plaintext_utf8))`，大小写不敏感（Emby 默认大写 hex，但容错）。
///
/// 其他格式直接返回 false；上层调用方负责在主 Argon2 校验失败时再走这一支。
pub fn verify_legacy_password(format: &str, stored_hash: &str, password: &str) -> bool {
    match format.to_ascii_lowercase().as_str() {
        "emby_sha1" => {
            let mut hasher = Sha1::new();
            hasher.update(password.as_bytes());
            let digest = hasher.finalize();
            let computed = hex::encode(digest);
            computed.eq_ignore_ascii_case(stored_hash.trim())
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emby_sha1_matches_known_vectors() {
        // 空字符串 SHA1
        assert!(verify_legacy_password(
            "emby_sha1",
            "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709",
            "",
        ));
        // "123456" SHA1
        assert!(verify_legacy_password(
            "emby_sha1",
            "7C4A8D09CA3762AF61E59520943DC26494F8941B",
            "123456",
        ));
        // 大小写无关 + 错误密码
        assert!(verify_legacy_password(
            "EMBY_SHA1",
            "7c4a8d09ca3762af61e59520943dc26494f8941b",
            "123456",
        ));
        assert!(!verify_legacy_password(
            "emby_sha1",
            "7C4A8D09CA3762AF61E59520943DC26494F8941B",
            "wrong",
        ));
    }

    #[test]
    fn unknown_format_returns_false() {
        assert!(!verify_legacy_password(
            "bcrypt",
            "$2y$10$abcdefg...",
            "anything"
        ));
    }
}
