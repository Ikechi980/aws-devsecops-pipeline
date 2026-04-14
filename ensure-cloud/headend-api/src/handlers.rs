use crate::{
    core_resources::ResidentPhotoResponse,
    error::AppError,
    identity::EnsureCommunity,
    models::{Community, Location, Resident},
    state::AppState,
};
use axum::{
    Json,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header::IF_NONE_MATCH},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

pub async fn core_community(
    State(state): State<AppState>,
    ensure: EnsureCommunity,
) -> Result<impl IntoResponse, AppError> {
    let core_id = resolve_core_community_id(&state, &ensure.ensure_community_id).await?;
    let community: Community = state.core_resources.get_community(&core_id).await?;
    Ok((StatusCode::OK, Json(community)))
}

pub async fn core_locations(
    State(state): State<AppState>,
    ensure: EnsureCommunity,
) -> Result<impl IntoResponse, AppError> {
    let core_id = resolve_core_community_id(&state, &ensure.ensure_community_id).await?;
    let locations: Vec<Location> = state.core_resources.get_locations(&core_id).await?;
    Ok((StatusCode::OK, Json(locations)))
}

pub async fn core_residents(
    State(state): State<AppState>,
    ensure: EnsureCommunity,
) -> Result<impl IntoResponse, AppError> {
    let core_id = resolve_core_community_id(&state, &ensure.ensure_community_id).await?;
    let residents: Vec<Resident> = state.core_resources.get_residents(&core_id).await?;
    Ok((StatusCode::OK, Json(residents)))
}

pub async fn core_resident_photo(
    State(state): State<AppState>,
    ensure: EnsureCommunity,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let core_id = resolve_core_community_id(&state, &ensure.ensure_community_id).await?;
    let if_none_match = headers.get(IF_NONE_MATCH).cloned();
    let photo = state
        .core_resources
        .get_resident_photo(&core_id, id, if_none_match)
        .await?;

    match photo {
        ResidentPhotoResponse::Ok(photo) => {
            let mut response = axum::response::Response::new(Body::from(photo.bytes));
            *response.status_mut() = StatusCode::OK;

            if let Ok(value) = axum::http::header::HeaderValue::from_str(&photo.content_type) {
                response
                    .headers_mut()
                    .insert(axum::http::header::CONTENT_TYPE, value);
            }
            if let Some(etag) = photo.etag
                && let Ok(value) = axum::http::header::HeaderValue::from_str(&etag)
            {
                response
                    .headers_mut()
                    .insert(axum::http::header::ETAG, value);
            }
            if let Some(last_modified) = photo.last_modified
                && let Ok(value) = axum::http::header::HeaderValue::from_str(&last_modified)
            {
                response
                    .headers_mut()
                    .insert(axum::http::header::LAST_MODIFIED, value);
            }

            Ok(response)
        }
        ResidentPhotoResponse::NotModified { etag } => {
            let mut response = axum::response::Response::new(Body::empty());
            *response.status_mut() = StatusCode::NOT_MODIFIED;
            if let Some(etag) = etag
                && let Ok(value) = axum::http::header::HeaderValue::from_str(&etag)
            {
                response
                    .headers_mut()
                    .insert(axum::http::header::ETAG, value);
            }
            Ok(response)
        }
    }
}

#[derive(Deserialize)]
pub struct EventsQuery {
    #[serde(rename = "payloadTypes")]
    payload_types: Option<String>,
    #[serde(rename = "afterDate")]
    after_date: Option<DateTime<Utc>>,
    #[serde(rename = "beforeDate")]
    before_date: Option<DateTime<Utc>>,
    limit: Option<u32>,
}

pub async fn events(
    State(state): State<AppState>,
    ensure: EnsureCommunity,
    Query(query): Query<EventsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let payload_types_str = query.payload_types.ok_or_else(|| {
        AppError::bad_request(
            "payload_types_missing",
            "payloadTypes is required and must not be empty",
        )
    })?;

    let payload_types: Vec<String> = payload_types_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    if payload_types.is_empty() {
        return Err(AppError::bad_request(
            "payload_types_missing",
            "payloadTypes is required and must not be empty",
        ));
    }

    let mut limit = query.limit.unwrap_or(state.events_limit_default);
    if limit > state.events_limit_max {
        limit = state.events_limit_max;
    }

    let events = state
        .events_repo
        .fetch_events(
            &ensure.ensure_community_id,
            &payload_types,
            query.after_date,
            query.before_date,
            limit,
        )
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "Failed to fetch events from MongoDB");
            AppError::bad_gateway("events_repo_error", "Failed to retrieve events")
        })?;

    Ok((StatusCode::OK, Json(events)))
}

async fn resolve_core_community_id(
    state: &AppState,
    ensure_community_id: &str,
) -> Result<String, AppError> {
    let core_id = state
        .systems
        .get_core_community_id(ensure_community_id)
        .await?
        .ok_or_else(|| {
            AppError::not_found(
                "core_community_mapping_missing",
                "No core community mapping exists for this community",
            )
        })?;

    Ok(core_id)
}
