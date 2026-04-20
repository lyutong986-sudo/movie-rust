use crate::{auth, error::AppError, naming, repository, state::AppState};
use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use url::{form_urlencoded, Url};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Videos/{item_id}/hls-proxy", get(proxy_hls_resource))
        .route("/Videos/{item_id}/{*stream_path}", get(stream_video))
        .route("/Items/{item_id}/File", get(stream_file))
        .route("/Items/{item_id}/Download", get(stream_file))
}

#[derive(Debug, Deserialize)]
struct VideoPath {
    item_id: Uuid,
    stream_path: String,
}

#[derive(Debug, Deserialize)]
struct HlsProxyQuery {
    target: String,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    _api_key: Option<String>,
}

async fn proxy_hls_resource(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<HlsProxyQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    ensure_http_url(&query.target)?;
    proxy_remote_stream(item_id, &query.target, request).await
}

async fn stream_video(
    State(state): State<AppState>,
    Path(path): Path<VideoPath>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let stream_path = path.stream_path.trim_start_matches('/');
    if let Some(subtitle_index) = parse_subtitle_stream_index(stream_path) {
        auth::require_auth(&state, request.headers(), request.uri().query()).await?;
        return serve_subtitle(&state, path.item_id, subtitle_index, request).await;
    }

    if !stream_path.starts_with("stream") {
        return Err(AppError::NotFound("视频流路径不存在".to_string()));
    }

    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_media_item(&state, path.item_id, request).await
}

async fn stream_file(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_media_item(&state, item_id, request).await
}

async fn serve_media_item(
    state: &AppState,
    item_id: Uuid,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    let path = PathBuf::from(&item.path);
    if !path.exists() {
        return Err(AppError::NotFound("媒体文件不存在".to_string()));
    }

    if naming::is_strm(&path) {
        let content = tokio::fs::read_to_string(&path).await?;
        let target = naming::strm_target_from_text(&content)
            .ok_or_else(|| AppError::BadRequest("STRM 文件没有有效的远程播放地址".to_string()))?;

        tracing::info!(
            item_id = %item.id,
            item_name = %item.name,
            target = %target,
            "代理 STRM 远程播放流"
        );
        return proxy_remote_stream(item.id, &target, request).await;
    }

    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}

async fn proxy_remote_stream(
    item_id: Uuid,
    url: &str,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let method = request.method().clone();
    let api_key = query_value(request.uri().query(), "api_key")
        .or_else(|| query_value(request.uri().query(), "ApiKey"))
        .or_else(|| query_value(request.uri().query(), "apiKey"));
    let client = Client::new();
    let mut remote_request = if method == Method::HEAD {
        client.head(url)
    } else {
        client.get(url)
    };

    for name in [
        header::RANGE,
        header::IF_RANGE,
        header::USER_AGENT,
        header::ACCEPT,
        header::ACCEPT_LANGUAGE,
    ] {
        if let Some(value) = request
            .headers()
            .get(&name)
            .and_then(|value| value.to_str().ok())
        {
            remote_request = remote_request.header(name.as_str(), value);
        }
    }

    let remote_response = remote_request
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("请求 STRM 远程流失败: {error}")))?;
    let status =
        StatusCode::from_u16(remote_response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = remote_response.headers().clone();

    tracing::info!(
        url = %url,
        status = %status,
        "STRM 远程流已响应"
    );

    if method != Method::HEAD && is_hls_manifest(url, &headers) {
        let text = remote_response
            .text()
            .await
            .map_err(|error| AppError::Internal(format!("读取 HLS 播放列表失败: {error}")))?;
        let rewritten = rewrite_hls_manifest(&text, url, item_id, api_key.as_deref())?;
        return Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(rewritten))
            .map_err(|error| AppError::Internal(format!("构建 HLS 播放列表响应失败: {error}")));
    }

    let mut response_builder = Response::builder().status(status);
    for (key, value) in headers.iter() {
        if is_hop_by_hop_header(key.as_str()) {
            continue;
        }

        if let Ok(value) = value.to_str() {
            response_builder = response_builder.header(key.as_str(), value);
        }
    }

    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(remote_response.bytes_stream())
    };

    response_builder
        .body(body)
        .map_err(|error| AppError::Internal(format!("构建 STRM 播放响应失败: {error}")))
}

async fn serve_subtitle(
    state: &AppState,
    item_id: Uuid,
    stream_index: i32,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let path = repository::subtitle_path_for_stream_index(&item, stream_index)
        .ok_or_else(|| AppError::NotFound("字幕不存在".to_string()))?;

    if !path.exists() {
        return Err(AppError::NotFound("字幕文件不存在".to_string()));
    }

    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}

fn parse_subtitle_stream_index(stream_path: &str) -> Option<i32> {
    let parts = stream_path.split('/').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }

    if !parts.get(1)?.eq_ignore_ascii_case("Subtitles") {
        return None;
    }

    parts.get(2)?.parse().ok()
}

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn ensure_http_url(value: &str) -> Result<(), AppError> {
    let url = Url::parse(value)
        .map_err(|_| AppError::BadRequest("HLS 代理地址不是有效 URL".to_string()))?;
    if matches!(url.scheme(), "http" | "https") {
        return Ok(());
    }

    Err(AppError::BadRequest("HLS 代理只允许 http/https 地址".to_string()))
}

fn query_value(query: Option<&str>, key: &str) -> Option<String> {
    form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find_map(|(candidate, value)| (candidate == key).then(|| value.into_owned()))
}

fn is_hls_manifest(url: &str, headers: &header::HeaderMap) -> bool {
    if naming::extension_from_url(url).is_some_and(|extension| extension == "m3u8") {
        return true;
    }

    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase)
        .is_some_and(|value| value.contains("mpegurl") || value.contains("m3u8"))
}

fn rewrite_hls_manifest(
    manifest: &str,
    manifest_url: &str,
    item_id: Uuid,
    api_key: Option<&str>,
) -> Result<String, AppError> {
    let base_url = Url::parse(manifest_url)
        .map_err(|_| AppError::BadRequest("HLS 播放列表地址不是有效 URL".to_string()))?;
    let mut rewritten = String::with_capacity(manifest.len());

    for line in manifest.lines() {
        if line.trim_start().starts_with("#EXT-X-") && line.contains("URI=\"") {
            rewritten.push_str(&rewrite_hls_tag_uri(line, &base_url, item_id, api_key));
        } else if line.trim().is_empty() || line.trim_start().starts_with('#') {
            rewritten.push_str(line);
        } else {
            rewritten.push_str(&proxied_hls_url(
                &resolve_hls_url(&base_url, line.trim())?,
                item_id,
                api_key,
            ));
        }
        rewritten.push('\n');
    }

    Ok(rewritten)
}

fn rewrite_hls_tag_uri(line: &str, base_url: &Url, item_id: Uuid, api_key: Option<&str>) -> String {
    let Some(uri_index) = line.find("URI=\"") else {
        return line.to_string();
    };
    let value_start = uri_index + 5;
    let Some(value_end_offset) = line[value_start..].find('"') else {
        return line.to_string();
    };
    let value_end = value_start + value_end_offset;
    let Ok(resolved) = resolve_hls_url(base_url, &line[value_start..value_end]) else {
        return line.to_string();
    };

    format!(
        "{}{}{}",
        &line[..value_start],
        proxied_hls_url(&resolved, item_id, api_key),
        &line[value_end..]
    )
}

fn resolve_hls_url(base_url: &Url, value: &str) -> Result<String, AppError> {
    let url = base_url
        .join(value)
        .map_err(|_| AppError::BadRequest("HLS 播放列表包含无效资源地址".to_string()))?;
    Ok(url.to_string())
}

fn proxied_hls_url(target: &str, item_id: Uuid, api_key: Option<&str>) -> String {
    let encoded_target: String = form_urlencoded::byte_serialize(target.as_bytes()).collect();
    let mut url = format!("/Videos/{item_id}/hls-proxy?target={encoded_target}");
    if let Some(api_key) = api_key {
        let encoded_key: String = form_urlencoded::byte_serialize(api_key.as_bytes()).collect();
        url.push_str("&api_key=");
        url.push_str(&encoded_key);
    }

    url
}
