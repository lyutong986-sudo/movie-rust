//! 用户名规范化与校验。
//!
//! 适用场景：注册 (`/Users/New`)、重命名 (`POST /Users/{Id}` Name 字段)、
//! 以及任何接受用户输入用户名的端点。Sakura embyboss bot 自己不做字符校验，
//! 后端是唯一的把关点。
//!
//! 规则（与 Sakura UI 提示「不限制中/英文/emoji，🚫特殊字符」对齐，但采取更
//! 精确的"不可见 / 危险字符黑名单"，而不是宽泛的"特殊字符黑名单"）：
//!
//! 1. **Unicode trim**：去掉首尾任何 [`char::is_whitespace`] 字符
//!    （包含 `U+00A0` 不间断空格、`U+3000` 全角空格、`U+200B` 零宽空格等）。
//! 2. **NFC 规范化**：用 [`UnicodeNormalization::nfc`] 把组合字符（如带音标的拉丁字母）
//!    统一为预组合形式，避免 `é` 的两种码点表达被当成不同用户。
//! 3. **长度上限 50 个 char**：与 Emby/Jellyfin 服务端官方默认一致。
//! 4. **拒绝控制字符** `U+0000..=U+001F` / `U+007F..=U+009F`：
//!    包含 NUL（Postgres 直接 500）、换行回车（污染日志和 Authorization header）、
//!    ANSI 转义（终端日志被染色注入）。
//! 5. **拒绝零宽 / 双向控制字符**：
//!    - `U+200B..=U+200F`（零宽空格 / 零宽 joiner / LTR/RTL marks）
//!    - `U+2028..=U+2029`（行 / 段落分隔符）
//!    - `U+202A..=U+202E`（双向覆写控制，可让"admin"显示成"nimda"伪装）
//!    - `U+2060..=U+2064` / `U+2066..=U+2069`（Word Joiner / Invisible Times / 隔离方向）
//!    - `U+FEFF`（BOM / Zero-Width No-Break Space）
//!
//! **不**禁止 emoji、CJK、空格分隔的多字段名（如 "Bob Smith"）、`. _ - + @ #` 等
//! 常见可见符号——这些是普通用户的合法用法。

use unicode_normalization::UnicodeNormalization;

use crate::error::AppError;

/// 用户名长度上限（按 Unicode `char` 计数，不是字节数）。
///
/// Emby 官方 Web UI 在 `dashboard/users/useredit.html` 上限是 50；
/// Jellyfin `Users.cs::ValidateUsername` 也是 `MaxLength = 50`。我们对齐。
pub const MAX_USERNAME_CHARS: usize = 50;

/// 校验并规范化用户名，给注册 / 重命名入口使用。
///
/// 失败返回 [`AppError::BadRequest`]，错误消息是给客户端看的中文文案，
/// Sakura 等 bot 可以直接转发给终端用户。
///
/// 调用方应**始终用本函数返回的 String 入库**，不要用原始输入。
pub fn normalize_and_validate(input: &str) -> Result<String, AppError> {
    // 第一步：Unicode trim。`str::trim` 只去 Pattern_White_Space + ASCII 空白，
    // 对 `U+00A0` / `U+3000` 这类常见全角空格无效，会留在 DB 里再也匹配不上。
    let trimmed: String = input.trim_matches(char::is_whitespace).to_string();

    if trimmed.is_empty() {
        return Err(AppError::BadRequest("用户名不能为空".to_string()));
    }

    // 第二步：先做"不可接受字符"扫描。NFC 规范化后再扫一次也不会少，但
    // 提前做能让错误消息按"原始字符"提示，对用户更友好。
    if let Some(reason) = scan_forbidden_chars(&trimmed) {
        return Err(AppError::BadRequest(reason));
    }

    // 第三步：NFC 规范化。注意必须在长度判断之前做，否则用户输入 `e\u{0301}`（NFD）
    // 会比 `é`（NFC）多算一个 char。
    let normalized: String = trimmed.nfc().collect();

    // 第四步：长度上限。
    let char_count = normalized.chars().count();
    if char_count > MAX_USERNAME_CHARS {
        return Err(AppError::BadRequest(format!(
            "用户名长度需在 1-{MAX_USERNAME_CHARS} 之间，当前 {char_count}"
        )));
    }

    // NFC 后应该不会重新引入控制字符，但保险起见再扫一次。
    if let Some(reason) = scan_forbidden_chars(&normalized) {
        return Err(AppError::BadRequest(reason));
    }

    Ok(normalized)
}

/// 用户查询（`get_user_by_name`、登录）输入端的轻量规范化。
///
/// 不做长度 / 字符黑名单校验（避免登录因为 client 多带空白就被 400 卡住），
/// 只做 trim + NFC，让 `lower(name) = lower($1)` 比较与入库时落到同一形式。
///
/// **不会** 返回错误：输入啥都尽量放行，让上层数据库查询负责说"用户不存在"。
pub fn normalize_for_lookup(input: &str) -> String {
    let trimmed: String = input.trim_matches(char::is_whitespace).nfc().collect();
    trimmed
}

fn scan_forbidden_chars(text: &str) -> Option<String> {
    for ch in text.chars() {
        if let Some(reason) = forbidden_reason(ch) {
            return Some(reason);
        }
    }
    None
}

fn forbidden_reason(ch: char) -> Option<String> {
    let code = ch as u32;
    // 控制字符 C0 / DEL / C1。
    if (0x00..=0x1F).contains(&code) || (0x7F..=0x9F).contains(&code) {
        return Some(format!(
            "用户名包含控制字符 (U+{code:04X})，不允许换行 / 制表符 / 终端转义等"
        ));
    }
    // 零宽 / 双向控制 / 行段落分隔。
    if matches!(
        code,
        0x200B..=0x200F
            | 0x2028..=0x2029
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x2069
            | 0xFEFF
    ) {
        return Some(format!(
            "用户名包含零宽 / 双向控制字符 (U+{code:04X})，可能用于仿冒，已拒绝"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_chinese() {
        assert_eq!(normalize_and_validate("小明").unwrap(), "小明");
    }

    #[test]
    fn accepts_emoji_and_spaces() {
        assert_eq!(
            normalize_and_validate("🎬 Bob Smith").unwrap(),
            "🎬 Bob Smith"
        );
    }

    #[test]
    fn trims_unicode_whitespace() {
        assert_eq!(
            normalize_and_validate("\u{3000}　alice\u{00A0}").unwrap(),
            "alice"
        );
    }

    #[test]
    fn nfc_normalizes_combining_chars() {
        // "é" 的两种码点形式应被规整成同一字符串。
        let nfd = "e\u{0301}le\u{0301}gant";
        let nfc = "élégant";
        assert_eq!(normalize_and_validate(nfd).unwrap(), nfc);
    }

    #[test]
    fn rejects_empty() {
        assert!(normalize_and_validate("").is_err());
        assert!(normalize_and_validate("   ").is_err());
        assert!(normalize_and_validate("\u{3000}\u{00A0}").is_err());
    }

    #[test]
    fn rejects_control_chars() {
        assert!(normalize_and_validate("alice\nbob").is_err());
        assert!(normalize_and_validate("alice\tbob").is_err());
        assert!(normalize_and_validate("alice\x00").is_err());
        assert!(normalize_and_validate("alice\x1b[31m").is_err());
    }

    #[test]
    fn rejects_zero_width_chars() {
        assert!(normalize_and_validate("alice\u{200B}").is_err());
        assert!(normalize_and_validate("\u{FEFF}alice").is_err());
        assert!(normalize_and_validate("alice\u{2060}bob").is_err());
    }

    #[test]
    fn rejects_bidi_override() {
        assert!(normalize_and_validate("admin\u{202E}txt").is_err());
        assert!(normalize_and_validate("\u{202A}guest").is_err());
    }

    #[test]
    fn rejects_overlong() {
        let s = "a".repeat(MAX_USERNAME_CHARS + 1);
        assert!(normalize_and_validate(&s).is_err());
    }

    #[test]
    fn accepts_50_char_emoji_limit() {
        let s: String = std::iter::repeat('🎬').take(MAX_USERNAME_CHARS).collect();
        assert_eq!(
            normalize_and_validate(&s).unwrap().chars().count(),
            MAX_USERNAME_CHARS
        );
    }

    #[test]
    fn lookup_keeps_loose() {
        // 登录路径不抛错，只规范化。
        assert_eq!(normalize_for_lookup("\u{3000}alice"), "alice");
        // 即便带零宽，也允许查询；DB 比较时自然会匹配不到，由 `用户不存在`
        // 走正常 401 流程。
        assert!(normalize_for_lookup("alice\u{200B}").contains('\u{200B}'));
    }
}
