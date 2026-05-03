//! Emby `playback_reporting` 第三方插件兼容层。
//!
//! `Sakura_embyboss` 等管理脚本用 `POST /user_usage_stats/submit_custom_query`
//! 提交一段 SQLite 风格的 SQL（基于 emby plugin 的 `PlaybackActivity` 表），期待返回：
//! ```json
//! {
//!   "colums":  ["UserId", "WatchTime"],
//!   "results": [["uid1", 1234], ["uid2", 567]],
//!   "message": "ok"
//! }
//! ```
//! （列名 `colums` 的拼写错误是 emby plugin 自身的，下游脚本就这么读，必须保持。）
//!
//! 本项目并未跑这个 plugin，但底层 `playback_events` + `sessions` + `media_items`
//! 三张表已经记下相同信息。这里把 Sakura 实际会发的 7 种固定 SQL pattern 用 regex 识别，
//! 翻译为 PostgreSQL 等价聚合，**不实现通用 SQL 引擎**（避免任意 SQL 注入风险）。
//!
//! 已识别的模式与 Sakura 函数对应：
//! 1. `emby_cust_commit(method='sp')`        —— 用户播放时长榜
//! 2. `emby_cust_commit(method=else)`        —— 单用户播放时长 + 上次登录
//! 3. `get_emby_userip`                      —— 用户的设备/IP 历史
//! 4. `get_emby_report`                      —— 媒体类型粒度报表
//! 5. `get_users_by_ip`                      —— 按 RemoteAddress 反查
//! 6. `get_users_by_device_name`             —— 按 DeviceName 模糊反查
//! 7. `get_users_by_client_name`             —— 按 ClientName 模糊反查
//! 8. `get_emby_user_devices`                —— 用户设备/IP 数排行（分页）
//!
//! 不识别的 SQL 一律返回空结果集 + `"message": "unsupported pattern"` 的 200，
//! 模仿 emby plugin "查询无结果" 的行为，避免 Sakura 因 4xx/5xx 直接 crash。

use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use chrono::{DateTime, FixedOffset, LocalResult, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::OnceLock;

use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/user_usage_stats/submit_custom_query", post(submit_custom_query))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct CustomQueryRequest {
    custom_query_string: String,
    #[serde(default)]
    replace_user_id: bool,
}

async fn submit_custom_query(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<CustomQueryRequest>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let sql = payload.custom_query_string.trim().to_string();
    if sql.is_empty() {
        return Ok(Json(empty_result("empty query")));
    }
    let normalized = normalize_sql(&sql);
    let result = dispatch_pattern(&state, &normalized, payload.replace_user_id).await?;
    Ok(Json(result))
}

/// 把 SQLite 风格的 SQL 压成单行小写、压缩空白，便于 regex 匹配。
fn normalize_sql(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

async fn dispatch_pattern(state: &AppState, sql: &str, replace_user_id: bool) -> Result<Value, AppError> {
    // 1. 用户播放时长榜：select userid, sum(playduration - pauseduration) ... group by userid
    if sql.contains("select userid")
        && sql.contains("sum(playduration - pauseduration)")
        && sql.contains("group by userid")
        && sql.contains("order by")
        && !sql.contains("itemtype")
        && !sql.contains("where userid =")
    {
        let (start, end) = extract_date_range(sql);
        return user_watchtime_ranking(state, start, end, replace_user_id).await;
    }

    // 2. 单用户播放时长 + 上次登录：max(datecreated) as lastlogin, ... where userid = ...
    if sql.contains("max(datecreated)")
        && sql.contains("sum(playduration - pauseduration)")
        && sql.contains("where userid =")
    {
        let user_id = extract_quoted_after(sql, "where userid = ").unwrap_or_default();
        let (start, end) = extract_date_range(sql);
        return single_user_watchtime(state, &user_id, start, end).await;
    }

    // 3. 用户设备/IP 历史：select devicename, clientname, remoteaddress ... where userid =
    if sql.contains("select devicename")
        && sql.contains("clientname")
        && sql.contains("remoteaddress")
        && sql.contains("where userid =")
        && !sql.contains("group by userid")
    {
        let user_id = extract_quoted_after(sql, "where userid = ").unwrap_or_default();
        return user_device_history(state, &user_id, replace_user_id).await;
    }

    // 4. 媒体类型粒度报表：select userid, itemid, itemtype, ... count(1) as play_count, sum(...) as total_duarion
    if sql.contains("select userid, itemid, itemtype")
        && sql.contains("count(1) as play_count")
        && sql.contains("from playbackactivity")
        && sql.contains("where itemtype =")
    {
        let item_type = extract_quoted_after(sql, "where itemtype = ").unwrap_or_default();
        let (start, end) = extract_date_range(sql);
        let user_id = extract_quoted_after(sql, " and userid = ");
        let limit = extract_limit(sql).unwrap_or(10);
        return media_report(state, &item_type, user_id.as_deref(), start, end, limit).await;
    }

    // 5/6/7. 按 IP / DeviceName / ClientName 反查
    if sql.contains("select distinct userid")
        && sql.contains("max(datecreated) as lastactivity")
        && sql.contains("count(*) as activitycount")
    {
        let (start, end) = extract_date_range(sql);
        if let Some(addr) = extract_quoted_after(sql, "where remoteaddress = ") {
            return users_by_ip(state, &addr, start, end, replace_user_id).await;
        }
        if let Some(kw) = extract_quoted_like(sql, "where devicename like ") {
            return users_by_device_or_client(state, "device_name", &kw, start, end, replace_user_id)
                .await;
        }
        if let Some(kw) = extract_quoted_like(sql, "where clientname like ") {
            return users_by_device_or_client(state, "client", &kw, start, end, replace_user_id)
                .await;
        }
    }

    // 8. 用户设备/IP 数排行：count(distinct devicename || ... clientname) ... group by userid
    if sql.contains("count(distinct devicename")
        && sql.contains("count(distinct remoteaddress)")
        && sql.contains("group by userid")
    {
        let limit = extract_limit(sql).unwrap_or(20);
        let offset = extract_offset(sql).unwrap_or(0);
        return user_devices_ranking(state, limit, offset, replace_user_id).await;
    }

    Ok(empty_result("unsupported pattern"))
}

// ---------------------- pattern 实现 ---------------------- //

async fn user_watchtime_ranking(
    state: &AppState,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    replace_user_id: bool,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        r#"
        WITH grp AS (
            SELECT pe.user_id,
                   pe.item_id,
                   MAX(pe.created_at) AS last_at,
                   MIN(pe.created_at) AS first_at,
                   MAX(pe.position_ticks) FILTER (WHERE pe.position_ticks IS NOT NULL)
                       AS max_p,
                   MIN(pe.position_ticks) FILTER (WHERE pe.position_ticks IS NOT NULL)
                       AS min_p,
                   EXTRACT(EPOCH FROM (MAX(pe.created_at) - MIN(pe.created_at)))::bigint
                       AS wall_secs
              FROM playback_events pe
             WHERE ($1::timestamptz IS NULL OR pe.created_at >= $1)
               AND ($2::timestamptz IS NULL OR pe.created_at <  $2)
             GROUP BY pe.user_id,
                      COALESCE(NULLIF(trim(pe.play_session_id), ''),
                               NULLIF(trim(pe.session_id), ''),
                               ''),
                      pe.item_id
        ),
        contrib AS (
            SELECT user_id,
                   CASE WHEN COALESCE(max_p, 0) > COALESCE(min_p, 0)
                        THEN GREATEST(0,
                             (COALESCE(max_p, 0) - COALESCE(min_p, 0)))::bigint
                             / 10000000::bigint
                        ELSE wall_secs END::bigint AS watch_secs
              FROM grp
        )
        SELECT c.user_id::text                  AS user_id,
               COALESCE(u.name, '')             AS user_name,
               COALESCE(SUM(c.watch_secs), 0)::bigint AS watch_time
          FROM contrib c
          LEFT JOIN users u ON u.id = c.user_id
         WHERE (u.is_hidden = false OR u.is_hidden IS NULL)
           AND (u.is_disabled = false OR u.is_disabled IS NULL)
         GROUP BY c.user_id, u.name
         ORDER BY watch_time DESC
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(&state.pool)
    .await?;

    let results: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let raw_id: String = r.get("user_id");
            let user_name: String = r.get("user_name");
            let user_id = if replace_user_id && !user_name.is_empty() {
                user_name
            } else {
                emby_id_or_raw(&raw_id)
            };
            let watch_time: i64 = r.get("watch_time");
            json!([user_id, watch_time])
        })
        .collect();

    let user_col = if replace_user_id { "UserName" } else { "UserId" };
    Ok(json!({
        "colums":  [user_col, "WatchTime"],
        "results": results,
        "message": "ok"
    }))
}

async fn single_user_watchtime(
    state: &AppState,
    user_emby_id: &str,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<Value, AppError> {
    let Ok(uuid) = models::emby_id_to_uuid(user_emby_id) else {
        return Ok(json!({ "colums": [], "results": [], "message": "invalid user id" }));
    };
    let row = sqlx::query(
        r#"
        WITH grp AS (
            SELECT pe.item_id,
                   MAX(pe.created_at) AS last_at,
                   MIN(pe.created_at) AS first_at,
                   MAX(pe.position_ticks) FILTER (WHERE pe.position_ticks IS NOT NULL)
                       AS max_p,
                   MIN(pe.position_ticks) FILTER (WHERE pe.position_ticks IS NOT NULL)
                       AS min_p,
                   EXTRACT(EPOCH FROM (MAX(pe.created_at) - MIN(pe.created_at)))::bigint
                       AS wall_secs
              FROM playback_events pe
             WHERE pe.user_id = $1
               AND ($2::timestamptz IS NULL OR pe.created_at >= $2)
               AND ($3::timestamptz IS NULL OR pe.created_at <  $3)
             GROUP BY COALESCE(NULLIF(trim(pe.play_session_id), ''),
                                  NULLIF(trim(pe.session_id), ''),
                                  ''),
                      pe.item_id
        ),
        agg AS (
            SELECT MAX(last_at)                                                    AS last_login,
                   SUM(CASE WHEN COALESCE(max_p, 0) > COALESCE(min_p, 0)
                            THEN ((COALESCE(max_p, 0) - COALESCE(min_p, 0))::bigint)
                                 / 600000000::bigint
                            ELSE GREATEST(wall_secs, 0)::bigint / 60::bigint END
                       )::bigint AS watch_time_minutes
              FROM grp
        )
        SELECT last_login,
               COALESCE(watch_time_minutes, 0)::bigint AS watch_time_minutes
          FROM agg
        "#,
    )
    .bind(uuid)
    .bind(start)
    .bind(end)
    .fetch_one(&state.pool)
    .await?;

    let last_login: Option<DateTime<Utc>> = row.get("last_login");
    let watch_minutes: i64 = row.try_get("watch_time_minutes").unwrap_or(0);
    if last_login.is_none() {
        return Ok(empty_result("no data"));
    }
    let lastlogin_str = last_login
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default();
    Ok(json!({
        "colums":  ["LastLogin", "WatchTime"],
        "results": [[lastlogin_str, watch_minutes]],
        "message": "ok"
    }))
}

async fn user_device_history(
    state: &AppState,
    user_emby_id: &str,
    replace_user_id: bool,
) -> Result<Value, AppError> {
    let Ok(uuid) = models::emby_id_to_uuid(user_emby_id) else {
        return Ok(json!({ "colums": [], "results": [], "message": "invalid user id" }));
    };
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT
               COALESCE(s.device_name, '')      AS device_name,
               COALESCE(s.client, '')           AS client,
               COALESCE(s.remote_address, '')   AS remote_address
          FROM playback_events pe
          LEFT JOIN sessions s
            ON s.access_token = pe.session_id
         WHERE pe.user_id = $1
        "#,
    )
    .bind(uuid)
    .fetch_all(&state.pool)
    .await?;

    let _ = replace_user_id;
    let results: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let dn: String = r.get("device_name");
            let cl: String = r.get("client");
            let ra: String = r.get("remote_address");
            json!([dn, cl, ra])
        })
        .collect();
    Ok(json!({
        "colums":  ["DeviceName", "ClientName", "RemoteAddress"],
        "results": results,
        "message": "ok"
    }))
}

async fn media_report(
    state: &AppState,
    item_type: &str,
    user_emby_id: Option<&str>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Value, AppError> {
    let user_uuid = user_emby_id.and_then(|id| models::emby_id_to_uuid(id).ok());
    let lowered = item_type.to_ascii_lowercase();
    let normalized_type = match lowered.as_str() {
        "movie" => "Movie",
        "series" => "Series",
        "episode" => "Episode",
        _ => &lowered,
    };
    let rows = sqlx::query(
        r#"
        WITH session_durations AS (
            SELECT pe.user_id,
                   pe.item_id,
                   pe.session_id,
                   EXTRACT(EPOCH FROM (MAX(pe.created_at) - MIN(pe.created_at)))::bigint AS dur
              FROM playback_events pe
             WHERE pe.item_id IS NOT NULL
               AND ($1::timestamptz IS NULL OR pe.created_at >= $1)
               AND ($2::timestamptz IS NULL OR pe.created_at <  $2)
               AND ($3::uuid IS NULL OR pe.user_id = $3)
             GROUP BY pe.user_id, pe.item_id, pe.session_id
        )
        SELECT sd.user_id::text  AS user_id,
               sd.item_id::text  AS item_id,
               mi.item_type      AS item_type,
               mi.name           AS name,
               COUNT(*)::bigint  AS play_count,
               COALESCE(SUM(sd.dur), 0)::bigint AS total_duarion
          FROM session_durations sd
          JOIN media_items mi ON mi.id = sd.item_id
          LEFT JOIN users u ON u.id = sd.user_id
         WHERE mi.item_type = $4
           AND (u.is_hidden = false OR u.is_hidden IS NULL)
           AND (u.is_disabled = false OR u.is_disabled IS NULL)
         GROUP BY sd.user_id, sd.item_id, mi.item_type, mi.name
         ORDER BY total_duarion DESC
         LIMIT $5
        "#,
    )
    .bind(start)
    .bind(end)
    .bind(user_uuid)
    .bind(normalized_type)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    let results: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let user_id: String = r.get("user_id");
            let item_id: String = r.get("item_id");
            let item_type: String = r.get("item_type");
            let name: String = r.get("name");
            let play_count: i64 = r.get("play_count");
            let dur: i64 = r.get("total_duarion");
            json!([
                emby_id_or_raw(&user_id),
                emby_id_or_raw(&item_id),
                item_type,
                name,
                play_count,
                dur
            ])
        })
        .collect();
    Ok(json!({
        "colums":  ["UserId", "ItemId", "ItemType", "name", "play_count", "total_duarion"],
        "results": results,
        "message": "ok"
    }))
}

async fn users_by_ip(
    state: &AppState,
    ip: &str,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    replace_user_id: bool,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT pe.user_id::text                          AS user_id,
               COALESCE(u.name, '')                      AS user_name,
               COALESCE(s.device_name, '')               AS device_name,
               COALESCE(s.client, '')                    AS client,
               COALESCE(s.remote_address, '')            AS remote_address,
               MAX(pe.created_at)                        AS last_activity,
               COUNT(*)::bigint                          AS activity_count
          FROM playback_events pe
          JOIN sessions s ON s.access_token = pe.session_id
          LEFT JOIN users u ON u.id = pe.user_id
         WHERE s.remote_address = $1
           AND ($2::timestamptz IS NULL OR pe.created_at >= $2)
           AND ($3::timestamptz IS NULL OR pe.created_at <  $3)
         GROUP BY pe.user_id, u.name, s.device_name, s.client, s.remote_address
         ORDER BY last_activity DESC
        "#,
    )
    .bind(ip)
    .bind(start)
    .bind(end)
    .fetch_all(&state.pool)
    .await?;
    Ok(build_users_by_xxx(rows, replace_user_id))
}

async fn users_by_device_or_client(
    state: &AppState,
    column: &str,
    keyword: &str,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    replace_user_id: bool,
) -> Result<Value, AppError> {
    let pattern = format!("%{}%", keyword);
    let sql = format!(
        r#"
        SELECT pe.user_id::text                          AS user_id,
               COALESCE(u.name, '')                      AS user_name,
               COALESCE(s.device_name, '')               AS device_name,
               COALESCE(s.client, '')                    AS client,
               COALESCE(s.remote_address, '')            AS remote_address,
               MAX(pe.created_at)                        AS last_activity,
               COUNT(*)::bigint                          AS activity_count
          FROM playback_events pe
          JOIN sessions s ON s.access_token = pe.session_id
          LEFT JOIN users u ON u.id = pe.user_id
         WHERE s.{column} ILIKE $1
           AND ($2::timestamptz IS NULL OR pe.created_at >= $2)
           AND ($3::timestamptz IS NULL OR pe.created_at <  $3)
         GROUP BY pe.user_id, u.name, s.device_name, s.client, s.remote_address
         ORDER BY last_activity DESC
        "#,
        column = column
    );
    let rows = sqlx::query(&sql)
        .bind(pattern)
        .bind(start)
        .bind(end)
        .fetch_all(&state.pool)
        .await?;
    Ok(build_users_by_xxx(rows, replace_user_id))
}

fn build_users_by_xxx(rows: Vec<sqlx::postgres::PgRow>, replace_user_id: bool) -> Value {
    // PB28：第二轮审计 Q1——`ReplaceUserId` 是 Sakura 等下游约定的「列名仍叫 UserId，
    // 但内容换成用户名」语义。Pattern #1/#8 已支持，#5/#6/#7 之前没传透；这里和它们
    // 一致：当 `replace_user_id=true` 且 users.name 非空时，第一列输出 `user_name`，
    // 否则回落 `emby_id_or_raw(user_id)` 保留兼容性。
    let results: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let user_id: String = r.get("user_id");
            let user_name: String = r.get("user_name");
            let dn: String = r.get("device_name");
            let cl: String = r.get("client");
            let ra: String = r.get("remote_address");
            let la: Option<DateTime<Utc>> = r.get("last_activity");
            let cnt: i64 = r.get("activity_count");
            let la_str = la.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default();
            let id_field = if replace_user_id && !user_name.is_empty() {
                user_name
            } else {
                emby_id_or_raw(&user_id)
            };
            json!([id_field, dn, cl, ra, la_str, cnt])
        })
        .collect();
    json!({
        "colums":  ["UserId", "DeviceName", "ClientName", "RemoteAddress", "LastActivity", "ActivityCount"],
        "results": results,
        "message": "ok"
    })
}

async fn user_devices_ranking(
    state: &AppState,
    limit: i64,
    offset: i64,
    replace_user_id: bool,
) -> Result<Value, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT pe.user_id::text                                        AS user_id,
               COALESCE(u.name, '')                                    AS user_name,
               COUNT(DISTINCT COALESCE(s.device_name, '') || '|' || COALESCE(s.client, '')) AS device_count,
               COUNT(DISTINCT COALESCE(s.remote_address, ''))          AS ip_count
          FROM playback_events pe
          LEFT JOIN sessions s ON s.access_token = pe.session_id
          LEFT JOIN users u    ON u.id = pe.user_id
         WHERE (u.is_hidden = false OR u.is_hidden IS NULL)
           AND (u.is_disabled = false OR u.is_disabled IS NULL)
         GROUP BY pe.user_id, u.name
         ORDER BY device_count DESC
         LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let results: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let raw_id: String = r.get("user_id");
            let user_name: String = r.get("user_name");
            let user_id = if replace_user_id && !user_name.is_empty() {
                user_name
            } else {
                emby_id_or_raw(&raw_id)
            };
            let dc: i64 = r.get("device_count");
            let ic: i64 = r.get("ip_count");
            json!([user_id, dc, ic])
        })
        .collect();
    let user_col = if replace_user_id { "UserName" } else { "UserId" };
    Ok(json!({
        "colums":  [user_col, "device_count", "ip_count"],
        "results": results,
        "message": "ok"
    }))
}

// ---------------------- helpers ---------------------- //

fn empty_result(message: &str) -> Value {
    json!({ "colums": [], "results": [], "message": message })
}

fn emby_id_or_raw(uuid_text: &str) -> String {
    uuid::Uuid::parse_str(uuid_text)
        .map(|u| models::uuid_to_emby_guid(&u))
        .unwrap_or_else(|_| uuid_text.to_string())
}

fn extract_date_range(sql: &str) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    static R_GE: OnceLock<Regex> = OnceLock::new();
    static R_LT: OnceLock<Regex> = OnceLock::new();
    let r_ge = R_GE.get_or_init(|| Regex::new(r"datecreated >= '([^']+)'").unwrap());
    let r_lt = R_LT.get_or_init(|| Regex::new(r"datecreated (?:<|<=) '([^']+)'").unwrap());
    // Sakura_embyboss `emby_cust_commit` 以东八区 `datetime.now(tz)` 拼 SQL；
    // 若不按 UTC+8 解析会整体偏 8h，裁剪掉窗口内回放记录。
    let parse = |s: &str| -> Option<DateTime<Utc>> {
        let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()?;
        let offset = FixedOffset::east_opt(8 * 3600)?;
        match offset.from_local_datetime(&naive) {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
            LocalResult::None => None,
        }
    };
    let start = r_ge.captures(sql).and_then(|c| parse(c.get(1)?.as_str()));
    let end = r_lt.captures(sql).and_then(|c| parse(c.get(1)?.as_str()));
    (start, end)
}

fn extract_quoted_after(sql: &str, prefix: &str) -> Option<String> {
    let idx = sql.find(prefix)?;
    let rest = &sql[idx + prefix.len()..];
    let rest = rest.trim_start();
    if !rest.starts_with('\'') {
        return None;
    }
    let close = rest[1..].find('\'')?;
    Some(rest[1..1 + close].to_string())
}

fn extract_quoted_like(sql: &str, prefix: &str) -> Option<String> {
    let idx = sql.find(prefix)?;
    let rest = &sql[idx + prefix.len()..];
    let rest = rest.trim_start();
    if !rest.starts_with('\'') {
        return None;
    }
    let close = rest[1..].find('\'')?;
    let token = &rest[1..1 + close];
    Some(token.trim_matches('%').to_string())
}

fn extract_limit(sql: &str) -> Option<i64> {
    static R: OnceLock<Regex> = OnceLock::new();
    let r = R.get_or_init(|| Regex::new(r"limit\s+(\d+)").unwrap());
    r.captures(sql)
        .and_then(|c| c.get(1)?.as_str().parse().ok())
}

fn extract_offset(sql: &str) -> Option<i64> {
    static R: OnceLock<Regex> = OnceLock::new();
    let r = R.get_or_init(|| Regex::new(r"offset\s+(\d+)").unwrap());
    r.captures(sql)
        .and_then(|c| c.get(1)?.as_str().parse().ok())
}
