use crate::error::{AppError, ERR_FOREIGN_KEY_VIOLATION, ERR_UNIQUE_VIOLATION};
use crate::events::ChangeEvent;
use crate::handlers::validate_name;
use crate::models::{CreateLocation, Location, LocationType, UpdateLocation};
use crate::requester::Requester;
use crate::state::AppState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use uuid::Uuid;

pub async fn list(
    State(state): State<AppState>,
    axum::extract::Path(community_id): axum::extract::Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Single atomic query: verify community exists and fetch locations
    let rows = sqlx::query!(
        r#"SELECT 
            c.id as "community_id!",
            l.id as "location_id?", 
            l.name as "name?",
            l.location_type as "location_type?: LocationType",
            l.yardi_reference_id as "yardi_reference_id?"
           FROM communities c
           LEFT JOIN locations l ON l.community_id = c.id
           WHERE c.id = $1"#,
        community_id
    )
    .fetch_all(&state.pool)
    .await?;

    if rows.is_empty() {
        return Err(AppError::not_found(
            "community_not_found",
            "Community not found",
        ));
    }

    let locations: Vec<Location> = rows
        .into_iter()
        .filter_map(|r| {
            r.location_id.map(|id| Location {
                id,
                community_id: r.community_id,
                name: r.name.unwrap_or_default(),
                location_type: r
                    .location_type
                    .unwrap_or(crate::models::LocationType::Apartment),
                yardi_reference_id: r.yardi_reference_id,
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(locations)))
}

pub async fn get(
    State(state): State<AppState>,
    axum::extract::Path((community_id, location_id)): axum::extract::Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let location = sqlx::query_as!(
        Location,
        r#"SELECT 
            id, 
            community_id, 
            name, 
            location_type as "location_type: LocationType", 
            yardi_reference_id 
        FROM locations 
        WHERE id = $1 AND community_id = $2"#,
        location_id,
        community_id
    )
    .fetch_optional(&state.pool)
    .await?;

    match location {
        Some(l) => Ok((StatusCode::OK, Json(l))),
        None => Err(AppError::not_found("location_not_found", "Not Found")),
    }
}

pub async fn create(
    State(state): State<AppState>,
    axum::extract::Path(community_id): axum::extract::Path<Uuid>,
    requester: Requester,
    Json(body): Json<CreateLocation>,
) -> Result<impl IntoResponse, AppError> {
    let name = validate_name(body.name)?;
    let location_type = body.location_type.ok_or_else(|| {
        AppError::bad_request("missing_location_type", "location_type is required")
    })?;
    let yardi_reference_id = body
        .yardi_reference_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let id = Uuid::new_v4();

    let mut tx = state.pool.begin().await?;

    // Verify community exists and check if Yardi integration is configured
    let community = sqlx::query!(
        "SELECT id, yardi_org_id FROM communities WHERE id = $1",
        community_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("community_not_found", "Community not found"))?;

    // Validate yardi_reference_id can only be set if community has Yardi integration
    if yardi_reference_id.is_some() && community.yardi_org_id.is_none() {
        return Err(AppError::conflict(
            "yardi_integration_required",
            "Cannot set Yardi reference ID without Yardi integration configured on community",
        ));
    }

    let location = sqlx::query_as!(
        Location,
        r#"INSERT INTO locations (id, community_id, name, location_type, yardi_reference_id) 
           VALUES ($1, $2, $3, $4, $5) 
           RETURNING 
               id, 
               community_id, 
               name, 
               location_type as "location_type: LocationType", 
               yardi_reference_id"#,
        id,
        community_id,
        name,
        location_type as LocationType,
        yardi_reference_id
    )
    .fetch_one(&mut *tx)
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

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "location",
            location.id,
            ChangeEvent::Create {
                after: location.clone(),
            },
            requester,
        )
        .await;

    Ok((StatusCode::CREATED, Json(location)))
}

pub async fn update(
    State(state): State<AppState>,
    axum::extract::Path((community_id, location_id)): axum::extract::Path<(Uuid, Uuid)>,
    requester: Requester,
    Json(body): Json<UpdateLocation>,
) -> Result<impl IntoResponse, AppError> {
    let name = validate_name(body.name)?;
    let location_type = body.location_type.ok_or_else(|| {
        AppError::bad_request("missing_location_type", "location_type is required")
    })?;
    let yardi_reference_id = body
        .yardi_reference_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut tx = state.pool.begin().await?;

    let before = sqlx::query_as!(
        Location,
        r#"SELECT 
            id, 
            community_id, 
            name, 
            location_type as "location_type: LocationType", 
            yardi_reference_id 
        FROM locations 
        WHERE id = $1 AND community_id = $2 
        FOR UPDATE"#,
        location_id,
        community_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("location_not_found", "Not Found"))?;

    // Validate yardi_reference_id can only be set if community has Yardi integration
    if yardi_reference_id.is_some() {
        let community_has_yardi = sqlx::query_scalar!(
            "SELECT yardi_org_id IS NOT NULL as \"has_yardi!\" FROM communities WHERE id = $1",
            community_id
        )
        .fetch_one(&mut *tx)
        .await?;

        if !community_has_yardi {
            return Err(AppError::conflict(
                "yardi_integration_required",
                "Cannot set Yardi reference ID without Yardi integration configured on community",
            ));
        }
    }

    let after = sqlx::query_as!(
        Location,
        r#"UPDATE locations 
           SET name = $1, location_type = $2, yardi_reference_id = $3 
           WHERE id = $4 
           RETURNING 
               id, 
               community_id, 
               name, 
               location_type as "location_type: LocationType", 
               yardi_reference_id"#,
        name,
        location_type as LocationType,
        yardi_reference_id,
        location_id
    )
    .fetch_one(&mut *tx)
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

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "location",
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
    axum::extract::Path((community_id, location_id)): axum::extract::Path<(Uuid, Uuid)>,
    requester: Requester,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.pool.begin().await?;

    let before = sqlx::query_as!(
        Location,
        r#"SELECT 
            id, 
            community_id, 
            name, 
            location_type as "location_type: LocationType", 
            yardi_reference_id 
        FROM locations 
        WHERE id = $1 AND community_id = $2 
        FOR UPDATE"#,
        location_id,
        community_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("location_not_found", "Not Found"))?;

    sqlx::query!("DELETE FROM locations WHERE id = $1", location_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let Some(db_err) = e.as_database_error()
                && db_err.code().as_deref() == Some(ERR_FOREIGN_KEY_VIOLATION)
            {
                AppError::conflict(
                    "location_has_residents",
                    "Cannot delete location with associated residents",
                )
            } else {
                AppError::Sqlx(e)
            }
        })?;

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "location",
            before.id,
            ChangeEvent::Delete { before },
            requester,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}
