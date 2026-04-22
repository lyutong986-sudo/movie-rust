use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    error::AppError,
    metadata::{
        models::ExternalPersonSearchResult,
        person_service::PersonService,
    },
    models::uuid_to_emby_guid,
    state::AppState,
};

/// 搜索外部人物查询参数
#[derive(Debug, Deserialize)]
pub struct SearchExternalPersonQuery {
    pub provider: String,
    pub name: String,
}

/// 提取人物数据请求
#[derive(Debug, Deserialize)]
pub struct FetchPersonRequest {
    pub provider: String,
    pub external_id: String,
}

/// 搜索外部人物
pub async fn search_external_person(
    State(state): State<AppState>,
    Query(query): Query<SearchExternalPersonQuery>,
) -> Result<Json<Vec<ExternalPersonSearchResult>>, AppError> {
    let metadata_manager = state.metadata_manager
        .as_ref()
        .ok_or_else(|| AppError::Internal("Metadata manager not initialized".to_string()))?;
    
    let person_service = PersonService::new(state.pool.clone(), metadata_manager.clone());
    
    let results = person_service.search_external_person(&query.provider, &query.name).await?;
    
    Ok(Json(results))
}

/// 从外部源提取人物数据
pub async fn fetch_person_from_external(
    State(state): State<AppState>,
    Json(request): Json<FetchPersonRequest>,
) -> Result<Json<FetchPersonResponse>, AppError> {
    let metadata_manager = state.metadata_manager
        .as_ref()
        .ok_or_else(|| AppError::Internal("Metadata manager not initialized".to_string()))?;
    
    let person_service = PersonService::new(state.pool.clone(), metadata_manager.clone());
    
    let person_id = person_service.fetch_person_from_external(&request.provider, &request.external_id).await?;
    
    Ok(Json(FetchPersonResponse {
        person_id: uuid_to_emby_guid(&person_id),
        success: true,
        message: "Person data fetched successfully".to_string(),
    }))
}

/// 提取人物数据响应
#[derive(Debug, serde::Serialize)]
pub struct FetchPersonResponse {
    pub person_id: String,
    pub success: bool,
    pub message: String,
}



pub fn router() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/Metadata/Persons/Search", axum::routing::get(search_external_person))
        .route("/Metadata/Persons/Fetch", axum::routing::post(fetch_person_from_external))
}
