use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{require_admin, AuthSession},
    error::AppError,
    models::{emby_id_to_uuid, uuid_to_emby_guid},
    repository,
    state::AppState,
};

const COLLECTION_PREFIX: &str = "collection:";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Collections", post(create_collection))
        .route("/collections", post(create_collection))
        .route(
            "/Collections/{id}/Items",
            post(add_collection_items).delete(remove_collection_items),
        )
        .route(
            "/collections/{id}/items",
            post(add_collection_items).delete(remove_collection_items),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CreateCollectionQuery {
    #[serde(default, alias = "name", alias = "Name")]
    name: Option<String>,
    #[serde(default, alias = "ids", alias = "Ids")]
    ids: Option<String>,
    #[serde(default, alias = "parentId", alias = "ParentId")]
    parent_id: Option<String>,
    #[serde(default, alias = "isLocked", alias = "IsLocked")]
    is_locked: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CollectionItemsBody {
    #[serde(default, alias = "ids", alias = "Ids")]
    ids: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CollectionItemsQuery {
    #[serde(default, alias = "ids", alias = "Ids")]
    ids: Option<String>,
}

fn parse_id_list(raw: Option<&str>) -> Vec<Uuid> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .filter_map(|v| emby_id_to_uuid(v).ok())
        .collect()
}

fn collection_key(id: Uuid) -> String {
    format!("{COLLECTION_PREFIX}{id}")
}

async fn create_collection(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<CreateCollectionQuery>,
) -> Result<Json<Value>, AppError> {
    require_admin(&session)?;
    let name = query
        .name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 Name 参数".to_string()))?
        .to_string();

    let ids = parse_id_list(query.ids.as_deref());
    let id = Uuid::new_v4();
    let doc = json!({
        "Id": id,
        "Name": name,
        "ParentId": query.parent_id,
        "IsLocked": query.is_locked.unwrap_or(false),
        "CreatedBy": session.user_id,
        "CreatedAt": chrono::Utc::now().to_rfc3339(),
        "ItemIds": ids.iter().map(|i| i.to_string()).collect::<Vec<_>>(),
    });
    repository::set_setting_value(&state.pool, &collection_key(id), doc).await?;

    Ok(Json(json!({
        "Id": uuid_to_emby_guid(&id),
        "Name": name,
        "Type": "BoxSet",
        "CollectionType": "movies",
        "IsFolder": true,
        "ChildCount": ids.len(),
    })))
}

async fn add_collection_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<CollectionItemsQuery>,
    body: Option<Json<CollectionItemsBody>>,
) -> Result<StatusCode, AppError> {
    require_admin(&session)?;
    let id = emby_id_to_uuid(&id).map_err(|_| AppError::BadRequest("无效合集 Id".to_string()))?;
    let key = collection_key(id);

    let mut ids: Vec<Uuid> = query
        .ids
        .clone()
        .as_deref()
        .map(|v| parse_id_list(Some(v)))
        .unwrap_or_default();
    if let Some(Json(b)) = body {
        ids.extend(parse_id_list(b.ids.as_deref()));
    }
    if ids.is_empty() {
        return Err(AppError::BadRequest("缺少 Ids 参数".to_string()));
    }

    let mut doc = repository::get_setting_value(&state.pool, &key)
        .await?
        .ok_or_else(|| AppError::NotFound("合集不存在".to_string()))?;

    let arr = doc
        .get_mut("ItemIds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppError::Internal("合集结构损坏".to_string()))?;

    for id in ids {
        let s = id.to_string();
        if !arr.iter().any(|v| v.as_str() == Some(s.as_str())) {
            arr.push(Value::String(s));
        }
    }

    repository::set_setting_value(&state.pool, &key, doc).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_router_builds_without_conflicts() {
        let _ = router();
    }

    #[test]
    fn parse_id_list_accepts_emby_guid_and_uuid_format() {
        let u = uuid::Uuid::nil();
        let emby = u.simple().to_string();
        let mixed = format!("{emby}, {u}");
        let parsed = parse_id_list(Some(&mixed));
        assert_eq!(parsed.len(), 2);
        assert!(parsed.iter().all(|p| *p == u));
    }
}
async fn remove_collection_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<CollectionItemsQuery>,
) -> Result<StatusCode, AppError> {
    require_admin(&session)?;
    let id = emby_id_to_uuid(&id).map_err(|_| AppError::BadRequest("无效合集 Id".to_string()))?;
    let key = collection_key(id);

    let ids: std::collections::BTreeSet<String> = parse_id_list(query.ids.as_deref())
        .into_iter()
        .map(|u| u.to_string())
        .collect();
    if ids.is_empty() {
        return Err(AppError::BadRequest("缺少 Ids 参数".to_string()));
    }

    let mut doc = repository::get_setting_value(&state.pool, &key)
        .await?
        .ok_or_else(|| AppError::NotFound("合集不存在".to_string()))?;

    let arr = doc
        .get_mut("ItemIds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppError::Internal("合集结构损坏".to_string()))?;

    arr.retain(|v| !v.as_str().map(|s| ids.contains(s)).unwrap_or(false));

    repository::set_setting_value(&state.pool, &key, doc).await?;
    Ok(StatusCode::NO_CONTENT)
}
