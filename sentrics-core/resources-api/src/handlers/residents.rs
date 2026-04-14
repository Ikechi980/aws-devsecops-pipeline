use crate::error::{AppError, ERR_FOREIGN_KEY_VIOLATION, ERR_UNIQUE_VIOLATION};
use crate::events::ChangeEvent;
use crate::handlers::{validate_first_name, validate_last_name};
use crate::models::{
    CreateResident, ListResidentsParams, Resident, ResidentPhotoMetadata, UpdateResident,
};
use crate::requester::Requester;
use crate::state::AppState;
use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH, LAST_MODIFIED};
use axum::http::{HeaderMap, HeaderValue};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use sha2::{Digest, Sha256};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

const MAX_PHOTO_SIZE_BYTES: usize = 2 * 1024 * 1024;
const ALLOWED_PHOTO_CONTENT_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];

pub async fn list(
    State(state): State<AppState>,
    axum::extract::Path(community_id): axum::extract::Path<Uuid>,
    axum::extract::Query(params): axum::extract::Query<ListResidentsParams>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query(
        r#"SELECT
            c.id AS community_id,
            l.id AS location_id,
            r.id AS resident_id,
            r.location_id AS resident_location_id,
            r.community_id AS resident_community_id,
            r.first_name AS resident_first_name,
            r.last_name AS resident_last_name,
            r.yardi_reference_id,
            rp.sha256 AS photo_etag,
            rp.content_type AS photo_content_type,
            rp.size_bytes AS photo_size_bytes,
            rp.updated_at AS photo_updated_at
           FROM communities c
           LEFT JOIN locations l ON c.id = l.community_id AND ($2::uuid IS NULL OR l.id = $2)
           LEFT JOIN residents r ON l.id = r.location_id
           LEFT JOIN resident_photos rp ON rp.resident_id = r.id
           WHERE c.id = $1"#,
    )
    .bind(community_id)
    .bind(params.location_id)
    .fetch_all(&state.pool)
    .await?;

    if rows.is_empty() {
        return Err(AppError::not_found(
            "community_not_found",
            "Community not found",
        ));
    }

    if params.location_id.is_some()
        && rows
            .first()
            .and_then(|r| r.try_get::<Option<Uuid>, _>("location_id").ok())
            .flatten()
            .is_none()
    {
        return Err(AppError::bad_request(
            "location_not_found",
            "Location not found in this community",
        ));
    }

    let mut residents = Vec::new();
    for row in &rows {
        if row.try_get::<Option<Uuid>, _>("resident_id")?.is_some() {
            residents.push(resident_from_row(row)?);
        }
    }

    Ok((StatusCode::OK, Json(residents)))
}

pub async fn get(
    State(state): State<AppState>,
    axum::extract::Path((community_id, resident_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let resident = fetch_resident(&state.pool, community_id, resident_id)
        .await?
        .ok_or_else(|| AppError::not_found("resident_not_found", "Not Found"))?;

    Ok((StatusCode::OK, Json(resident)))
}

pub async fn create(
    State(state): State<AppState>,
    axum::extract::Path(community_id): axum::extract::Path<Uuid>,
    requester: Requester,
    Json(body): Json<CreateResident>,
) -> Result<impl IntoResponse, AppError> {
    let first_name = validate_first_name(body.first_name)?;
    let last_name = validate_last_name(body.last_name)?;
    let location_id = body
        .location_id
        .ok_or_else(|| AppError::bad_request("location_id_required", "Location ID is required"))?;
    let yardi_reference_id = body
        .yardi_reference_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let id = Uuid::new_v4();

    let mut tx = state.pool.begin().await?;

    let location_info = sqlx::query(
        r#"SELECT l.id, l.community_id, c.yardi_org_id
           FROM locations l
           JOIN communities c ON l.community_id = c.id
           WHERE l.id = $1 AND l.community_id = $2"#,
    )
    .bind(location_id)
    .bind(community_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        AppError::bad_request("location_not_found", "Location not found in this community")
    })?;

    if yardi_reference_id.is_some()
        && location_info
            .try_get::<Option<String>, _>("yardi_org_id")?
            .is_none()
    {
        return Err(AppError::conflict(
            "yardi_integration_required",
            "Cannot set Yardi reference ID without Yardi integration configured on community",
        ));
    }

    sqlx::query(
        r#"INSERT INTO residents (id, location_id, community_id, first_name, last_name, yardi_reference_id)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(id)
    .bind(location_id)
    .bind(community_id)
    .bind(first_name)
    .bind(last_name)
    .bind(yardi_reference_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if let Some(db_err) = e.as_database_error()
            && db_err.code().as_deref() == Some(ERR_UNIQUE_VIOLATION)
        {
            AppError::conflict(
                "yardi_reference_id_conflict",
                "Yardi reference ID already exists in this community",
            )
        } else {
            AppError::Sqlx(e)
        }
    })?;

    let resident = fetch_resident_with_tx(&mut tx, community_id, id)
        .await?
        .ok_or_else(|| {
            AppError::internal_server_error(
                "resident_load_failed",
                "Failed to load created resident",
            )
        })?;

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "resident",
            resident.id,
            ChangeEvent::Create {
                after: resident.clone(),
            },
            requester,
        )
        .await;

    Ok((StatusCode::CREATED, Json(resident)))
}

pub async fn update(
    State(state): State<AppState>,
    axum::extract::Path((community_id, resident_id)): axum::extract::Path<(Uuid, Uuid)>,
    requester: Requester,
    Json(body): Json<UpdateResident>,
) -> Result<impl IntoResponse, AppError> {
    let first_name = validate_first_name(body.first_name)?;
    let last_name = validate_last_name(body.last_name)?;
    let location_id = body
        .location_id
        .ok_or_else(|| AppError::bad_request("location_id_required", "Location ID is required"))?;
    let yardi_reference_id = body
        .yardi_reference_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut tx = state.pool.begin().await?;

    let before = fetch_resident_with_tx_for_update(&mut tx, community_id, resident_id)
        .await?
        .ok_or_else(|| AppError::not_found("resident_not_found", "Not Found"))?;

    let location_info = sqlx::query(
        r#"SELECT l.id, c.yardi_org_id
           FROM locations l
           JOIN communities c ON l.community_id = c.id
           WHERE l.id = $1 AND l.community_id = $2"#,
    )
    .bind(location_id)
    .bind(community_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        AppError::bad_request("location_not_found", "Location not found in this community")
    })?;

    if yardi_reference_id.is_some()
        && location_info
            .try_get::<Option<String>, _>("yardi_org_id")?
            .is_none()
    {
        return Err(AppError::conflict(
            "yardi_integration_required",
            "Cannot set Yardi reference ID without Yardi integration configured on community",
        ));
    }

    sqlx::query(
        r#"UPDATE residents
           SET first_name = $1, last_name = $2, location_id = $3, yardi_reference_id = $4
           WHERE id = $5"#,
    )
    .bind(first_name)
    .bind(last_name)
    .bind(location_id)
    .bind(yardi_reference_id)
    .bind(resident_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if let Some(db_err) = e.as_database_error()
            && db_err.code().as_deref() == Some(ERR_UNIQUE_VIOLATION)
        {
            AppError::conflict(
                "yardi_reference_id_conflict",
                "Yardi reference ID already exists in this community",
            )
        } else {
            AppError::Sqlx(e)
        }
    })?;

    let after = fetch_resident_with_tx(&mut tx, community_id, resident_id)
        .await?
        .ok_or_else(|| {
            AppError::internal_server_error(
                "resident_load_failed",
                "Failed to load updated resident",
            )
        })?;

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "resident",
            before.id,
            ChangeEvent::Update {
                before,
                after: after.clone(),
            },
            requester,
        )
        .await;

    Ok((StatusCode::OK, Json(after)))
}

pub async fn delete(
    State(state): State<AppState>,
    axum::extract::Path((community_id, resident_id)): axum::extract::Path<(Uuid, Uuid)>,
    requester: Requester,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.pool.begin().await?;

    let before = fetch_resident_with_tx_for_update(&mut tx, community_id, resident_id)
        .await?
        .ok_or_else(|| AppError::not_found("resident_not_found", "Not Found"))?;

    sqlx::query("DELETE FROM residents WHERE id = $1")
        .bind(resident_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let Some(db_err) = e.as_database_error()
                && db_err.code().as_deref() == Some(ERR_FOREIGN_KEY_VIOLATION)
            {
                AppError::conflict(
                    "resident_has_dependencies",
                    "Cannot delete resident with dependencies",
                )
            } else {
                AppError::Sqlx(e)
            }
        })?;

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "resident",
            before.id,
            ChangeEvent::Delete { before },
            requester,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_photo(
    State(state): State<AppState>,
    axum::extract::Path((community_id, resident_id)): axum::extract::Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query(
        r#"SELECT
            r.id AS resident_id,
            rp.content_type,
            rp.image_data,
            rp.sha256,
            rp.updated_at
           FROM residents r
           LEFT JOIN resident_photos rp ON rp.resident_id = r.id
           WHERE r.id = $1 AND r.community_id = $2"#,
    )
    .bind(resident_id)
    .bind(community_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("resident_not_found", "Not Found"))?;

    let content_type = row
        .try_get::<Option<String>, _>("content_type")?
        .ok_or_else(|| {
            AppError::not_found("resident_photo_not_found", "Resident photo not found")
        })?;
    let image_data = row
        .try_get::<Option<Vec<u8>>, _>("image_data")?
        .ok_or_else(|| {
            AppError::not_found("resident_photo_not_found", "Resident photo not found")
        })?;
    let sha256 = row.try_get::<Option<String>, _>("sha256")?.ok_or_else(|| {
        AppError::not_found("resident_photo_not_found", "Resident photo not found")
    })?;
    let updated_at = row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("updated_at")?
        .ok_or_else(|| {
            AppError::not_found("resident_photo_not_found", "Resident photo not found")
        })?;

    let etag = format!("sha256:{sha256}");
    if if_none_match_matches(&headers, &etag) {
        let mut response = axum::response::Response::new(Body::empty());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        response.headers_mut().insert(
            ETAG,
            HeaderValue::from_str(&format!("\"{etag}\"")).map_err(|_| {
                AppError::internal_server_error(
                    "resident_photo_header_invalid",
                    "Failed to build photo response headers",
                )
            })?,
        );
        return Ok(response);
    }

    let mut response = axum::response::Response::new(Body::from(image_data));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&content_type).map_err(|_| {
            AppError::internal_server_error(
                "resident_photo_header_invalid",
                "Failed to build photo response headers",
            )
        })?,
    );
    response.headers_mut().insert(
        ETAG,
        HeaderValue::from_str(&format!("\"{etag}\"")).map_err(|_| {
            AppError::internal_server_error(
                "resident_photo_header_invalid",
                "Failed to build photo response headers",
            )
        })?,
    );
    response.headers_mut().insert(
        LAST_MODIFIED,
        HeaderValue::from_str(&updated_at.to_rfc2822()).map_err(|_| {
            AppError::internal_server_error(
                "resident_photo_header_invalid",
                "Failed to build photo response headers",
            )
        })?,
    );

    Ok(response)
}

pub async fn put_photo(
    State(state): State<AppState>,
    axum::extract::Path((community_id, resident_id)): axum::extract::Path<(Uuid, Uuid)>,
    requester: Requester,
    headers: HeaderMap,
    request: Request,
) -> Result<impl IntoResponse, AppError> {
    let content_type = normalize_and_validate_content_type(&headers)?;

    if headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|len| len > MAX_PHOTO_SIZE_BYTES)
    {
        return Err(AppError::payload_too_large(
            "resident_photo_too_large",
            "Resident photo exceeds maximum size of 2 MB",
        ));
    }

    let body = to_bytes(request.into_body(), MAX_PHOTO_SIZE_BYTES + 1)
        .await
        .map_err(|_| {
            AppError::payload_too_large(
                "resident_photo_too_large",
                "Resident photo exceeds maximum size of 2 MB",
            )
        })?;

    if body.is_empty() {
        return Err(AppError::bad_request(
            "resident_photo_empty",
            "Resident photo body cannot be empty",
        ));
    }

    if body.len() > MAX_PHOTO_SIZE_BYTES {
        return Err(AppError::payload_too_large(
            "resident_photo_too_large",
            "Resident photo exceeds maximum size of 2 MB",
        ));
    }

    let digest = Sha256::digest(&body);
    let mut sha256 = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut sha256, "{byte:02x}").expect("writing to String must succeed");
    }

    let mut tx = state.pool.begin().await?;

    let before = fetch_resident_with_tx_for_update(&mut tx, community_id, resident_id)
        .await?
        .ok_or_else(|| AppError::not_found("resident_not_found", "Not Found"))?;

    let existing =
        sqlx::query("SELECT sha256, content_type FROM resident_photos WHERE resident_id = $1")
            .bind(resident_id)
            .fetch_optional(&mut *tx)
            .await?;

    let unchanged = existing.as_ref().is_some_and(|row| {
        row.try_get::<String, _>("sha256").ok().as_deref() == Some(sha256.as_str())
            && row.try_get::<String, _>("content_type").ok().as_deref()
                == Some(content_type.as_str())
    });

    if !unchanged {
        sqlx::query(
            r#"INSERT INTO resident_photos (resident_id, content_type, image_data, sha256, source_last_updated, updated_at)
               VALUES ($1, $2, $3, $4, NULL, now())
               ON CONFLICT (resident_id)
               DO UPDATE SET
                   content_type = EXCLUDED.content_type,
                   image_data = EXCLUDED.image_data,
                   sha256 = EXCLUDED.sha256,
                   source_last_updated = NULL,
                   updated_at = now()"#,
        )
        .bind(resident_id)
        .bind(content_type.as_str())
        .bind(body.to_vec())
        .bind(sha256)
        .execute(&mut *tx)
        .await?;
    }

    let after = fetch_resident_with_tx(&mut tx, community_id, resident_id)
        .await?
        .ok_or_else(|| {
            AppError::internal_server_error(
                "resident_load_failed",
                "Failed to load resident after photo update",
            )
        })?;

    tx.commit().await?;

    if !unchanged {
        let _ = state
            .publisher
            .publish(
                "resident",
                before.id,
                ChangeEvent::Update {
                    before,
                    after: after.clone(),
                },
                requester,
            )
            .await;
    }

    Ok((StatusCode::OK, Json(after)))
}

pub async fn delete_photo(
    State(state): State<AppState>,
    axum::extract::Path((community_id, resident_id)): axum::extract::Path<(Uuid, Uuid)>,
    requester: Requester,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.pool.begin().await?;

    let before = fetch_resident_with_tx_for_update(&mut tx, community_id, resident_id)
        .await?
        .ok_or_else(|| AppError::not_found("resident_not_found", "Not Found"))?;

    if before.photo.is_none() {
        return Err(AppError::not_found(
            "resident_photo_not_found",
            "Resident photo not found",
        ));
    }

    sqlx::query("DELETE FROM resident_photos WHERE resident_id = $1")
        .bind(resident_id)
        .execute(&mut *tx)
        .await?;

    let after = fetch_resident_with_tx(&mut tx, community_id, resident_id)
        .await?
        .ok_or_else(|| {
            AppError::internal_server_error(
                "resident_load_failed",
                "Failed to load resident after photo deletion",
            )
        })?;

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "resident",
            before.id,
            ChangeEvent::Update { before, after },
            requester,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

fn normalize_and_validate_content_type(headers: &HeaderMap) -> Result<String, AppError> {
    let raw = headers
        .get(CONTENT_TYPE)
        .ok_or_else(|| {
            AppError::bad_request(
                "resident_photo_content_type_required",
                "Content-Type header is required for resident photo upload",
            )
        })?
        .to_str()
        .map_err(|_| {
            AppError::bad_request(
                "resident_photo_content_type_invalid",
                "Content-Type header is invalid",
            )
        })?;

    let content_type = raw
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if !ALLOWED_PHOTO_CONTENT_TYPES.contains(&content_type.as_str()) {
        return Err(AppError::bad_request(
            "resident_photo_content_type_invalid",
            "Resident photo content type must be image/jpeg, image/png, or image/webp",
        ));
    }

    Ok(content_type)
}

fn resident_from_row(row: &PgRow) -> Result<Resident, sqlx::Error> {
    let photo_etag = row.try_get::<Option<String>, _>("photo_etag")?;
    let photo = match photo_etag {
        Some(hash) => Some(ResidentPhotoMetadata {
            etag: format!("sha256:{hash}"),
            content_type: row.try_get::<String, _>("photo_content_type")?,
            size_bytes: row.try_get::<i32, _>("photo_size_bytes")? as i64,
            updated_at: row.try_get::<chrono::DateTime<chrono::Utc>, _>("photo_updated_at")?,
        }),
        None => None,
    };

    Ok(Resident {
        id: row.try_get("resident_id")?,
        location_id: row.try_get("resident_location_id")?,
        community_id: row.try_get("resident_community_id")?,
        first_name: row.try_get("resident_first_name")?,
        last_name: row.try_get("resident_last_name")?,
        yardi_reference_id: row.try_get("yardi_reference_id")?,
        photo,
    })
}

async fn fetch_resident(
    pool: &sqlx::PgPool,
    community_id: Uuid,
    resident_id: Uuid,
) -> Result<Option<Resident>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT
            r.id AS resident_id,
            r.location_id AS resident_location_id,
            r.community_id AS resident_community_id,
            r.first_name AS resident_first_name,
            r.last_name AS resident_last_name,
            r.yardi_reference_id,
            rp.sha256 AS photo_etag,
            rp.content_type AS photo_content_type,
            rp.size_bytes AS photo_size_bytes,
            rp.updated_at AS photo_updated_at
           FROM residents r
           LEFT JOIN resident_photos rp ON rp.resident_id = r.id
           WHERE r.id = $1 AND r.community_id = $2"#,
    )
    .bind(resident_id)
    .bind(community_id)
    .fetch_optional(pool)
    .await?;

    row.as_ref().map(resident_from_row).transpose()
}

async fn fetch_resident_with_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    community_id: Uuid,
    resident_id: Uuid,
) -> Result<Option<Resident>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT
            r.id AS resident_id,
            r.location_id AS resident_location_id,
            r.community_id AS resident_community_id,
            r.first_name AS resident_first_name,
            r.last_name AS resident_last_name,
            r.yardi_reference_id,
            rp.sha256 AS photo_etag,
            rp.content_type AS photo_content_type,
            rp.size_bytes AS photo_size_bytes,
            rp.updated_at AS photo_updated_at
           FROM residents r
           LEFT JOIN resident_photos rp ON rp.resident_id = r.id
           WHERE r.id = $1 AND r.community_id = $2"#,
    )
    .bind(resident_id)
    .bind(community_id)
    .fetch_optional(&mut **tx)
    .await?;

    row.as_ref().map(resident_from_row).transpose()
}

async fn fetch_resident_with_tx_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    community_id: Uuid,
    resident_id: Uuid,
) -> Result<Option<Resident>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT
            r.id AS resident_id,
            r.location_id AS resident_location_id,
            r.community_id AS resident_community_id,
            r.first_name AS resident_first_name,
            r.last_name AS resident_last_name,
            r.yardi_reference_id,
            rp.sha256 AS photo_etag,
           rp.content_type AS photo_content_type,
           rp.size_bytes AS photo_size_bytes,
           rp.updated_at AS photo_updated_at
           FROM residents r
           LEFT JOIN resident_photos rp ON rp.resident_id = r.id
           WHERE r.id = $1 AND r.community_id = $2
           FOR UPDATE OF r"#,
    )
    .bind(resident_id)
    .bind(community_id)
    .fetch_optional(&mut **tx)
    .await?;

    row.as_ref().map(resident_from_row).transpose()
}

fn if_none_match_matches(headers: &HeaderMap, etag: &str) -> bool {
    let raw = match headers.get(IF_NONE_MATCH).and_then(|v| v.to_str().ok()) {
        Some(value) => value,
        None => return false,
    };

    raw.split(',').any(|candidate| {
        let trimmed = candidate.trim();
        let without_weak = trimmed.strip_prefix("W/").unwrap_or(trimmed);
        let unquoted = without_weak.trim_matches('"');
        unquoted == etag
    })
}
