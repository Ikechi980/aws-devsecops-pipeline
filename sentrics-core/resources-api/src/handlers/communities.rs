use crate::error::{AppError, ERR_FOREIGN_KEY_VIOLATION};
use crate::events::ChangeEvent;
use crate::handlers::validate_name;
use crate::models::{Community, CreateCommunity, UpdateCommunity};
use crate::requester::Requester;
use crate::state::AppState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use url::Url;
use uuid::Uuid;

type YardiFields = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

pub async fn list(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let communities = sqlx::query_as!(
        Community,
        "SELECT id, name, yardi_org_id, yardi_api_key, yardi_api_secret, yardi_api_base_url, yardi_token_url FROM communities"
    )
    .fetch_all(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(communities)))
}

pub async fn get(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let community = sqlx::query_as!(
        Community,
        "SELECT id, name, yardi_org_id, yardi_api_key, yardi_api_secret, yardi_api_base_url, yardi_token_url FROM communities WHERE id = $1",
        id
    )
    .fetch_optional(&state.pool)
    .await?;

    match community {
        Some(c) => Ok((StatusCode::OK, Json(c))),
        None => Err(AppError::not_found("community_not_found", "Not Found")),
    }
}

pub async fn create(
    State(state): State<AppState>,
    requester: Requester,
    Json(body): Json<CreateCommunity>,
) -> Result<impl IntoResponse, AppError> {
    let name = validate_name(body.name)?;
    let (yardi_org_id, yardi_api_key, yardi_api_secret, yardi_api_base_url, yardi_token_url) =
        validate_yardi_fields(
            &body.yardi_org_id,
            &body.yardi_api_key,
            &body.yardi_api_secret,
            &body.yardi_api_base_url,
            &body.yardi_token_url,
        )?;
    let id = Uuid::new_v4();

    let mut tx = state.pool.begin().await?;

    let community = sqlx::query_as!(
        Community,
        r#"INSERT INTO communities (
               id,
               name,
               yardi_org_id,
               yardi_api_key,
               yardi_api_secret,
               yardi_api_base_url,
               yardi_token_url
           ) 
           VALUES ($1, $2, $3, $4, $5, $6, $7) 
           RETURNING id, name, yardi_org_id, yardi_api_key, yardi_api_secret, yardi_api_base_url, yardi_token_url"#,
        id,
        name,
        yardi_org_id,
        yardi_api_key,
        yardi_api_secret,
        yardi_api_base_url,
        yardi_token_url
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "community",
            community.id,
            ChangeEvent::Create {
                after: community.clone(),
            },
            requester,
        )
        .await;

    Ok((StatusCode::CREATED, Json(community)))
}

pub async fn update(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    requester: Requester,
    Json(body): Json<UpdateCommunity>,
) -> Result<impl IntoResponse, AppError> {
    let name = validate_name(body.name)?;
    let (yardi_org_id, yardi_api_key, yardi_api_secret, yardi_api_base_url, yardi_token_url) =
        validate_yardi_fields(
            &body.yardi_org_id,
            &body.yardi_api_key,
            &body.yardi_api_secret,
            &body.yardi_api_base_url,
            &body.yardi_token_url,
        )?;

    let mut tx = state.pool.begin().await?;

    let before = sqlx::query_as!(
        Community,
        "SELECT id, name, yardi_org_id, yardi_api_key, yardi_api_secret, yardi_api_base_url, yardi_token_url FROM communities WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("community_not_found", "Not Found"))?;

    // If Yardi fields are being unset, check for existing references
    let had_yardi = before.yardi_org_id.is_some();
    let will_have_yardi = yardi_org_id.is_some();

    if had_yardi && !will_have_yardi {
        let has_yardi_references = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM locations WHERE community_id = $1 AND yardi_reference_id IS NOT NULL
                UNION ALL
                SELECT 1 FROM residents WHERE community_id = $1 AND yardi_reference_id IS NOT NULL
            ) as "exists!""#,
            id
        )
        .fetch_one(&mut *tx)
        .await?;

        if has_yardi_references {
            return Err(AppError::conflict(
                "yardi_references_present",
                "Cannot unset Yardi integration while locations or residents have Yardi reference IDs",
            ));
        }
    }

    let after = sqlx::query_as!(
        Community,
        r#"UPDATE communities 
           SET name = $1,
               yardi_org_id = $2,
               yardi_api_key = $3,
               yardi_api_secret = $4,
               yardi_api_base_url = $5,
               yardi_token_url = $6
           WHERE id = $7 
           RETURNING id, name, yardi_org_id, yardi_api_key, yardi_api_secret, yardi_api_base_url, yardi_token_url"#,
        name,
        yardi_org_id,
        yardi_api_key,
        yardi_api_secret,
        yardi_api_base_url,
        yardi_token_url,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "community",
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
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    requester: Requester,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.pool.begin().await?;

    let before = sqlx::query_as!(
        Community,
        "SELECT id, name, yardi_org_id, yardi_api_key, yardi_api_secret, yardi_api_base_url, yardi_token_url FROM communities WHERE id = $1 FOR UPDATE",
        id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("community_not_found", "Not Found"))?;

    sqlx::query!("DELETE FROM communities WHERE id = $1", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let Some(db_err) = e.as_database_error()
                && db_err.code().as_deref() == Some(ERR_FOREIGN_KEY_VIOLATION)
            {
                AppError::conflict(
                    "community_has_locations",
                    "Cannot delete community with associated locations",
                )
            } else {
                AppError::Sqlx(e)
            }
        })?;

    tx.commit().await?;

    let _ = state
        .publisher
        .publish(
            "community",
            before.id,
            ChangeEvent::Delete { before },
            requester,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Validates that Yardi integration fields are either all set or all unset.
fn validate_yardi_fields(
    org_id: &Option<String>,
    api_key: &Option<String>,
    api_secret: &Option<String>,
    api_base_url: &Option<String>,
    token_url: &Option<String>,
) -> Result<YardiFields, AppError> {
    let org_id = org_id.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    let api_key = api_key.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty());
    let api_secret = api_secret
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let api_base_url = api_base_url
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let token_url = token_url
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let all_set = org_id.is_some()
        && api_key.is_some()
        && api_secret.is_some()
        && api_base_url.is_some()
        && token_url.is_some();
    let none_set = org_id.is_none()
        && api_key.is_none()
        && api_secret.is_none()
        && api_base_url.is_none()
        && token_url.is_none();

    if all_set || none_set {
        let api_base_url = api_base_url.map(validate_yardi_api_base_url).transpose()?;
        let token_url = token_url.map(validate_yardi_token_url).transpose()?;

        Ok((
            org_id.map(|s| s.to_string()),
            api_key.map(|s| s.to_string()),
            api_secret.map(|s| s.to_string()),
            api_base_url,
            token_url,
        ))
    } else {
        Err(AppError::bad_request(
            "yardi_fields_incomplete",
            "Yardi integration fields must be all set or all unset",
        ))
    }
}

fn validate_yardi_api_base_url(value: &str) -> Result<String, AppError> {
    let url = validate_yardi_url(value, "yardi_api_base_url_invalid", "Yardi API base URL")?;

    if url.query().is_some() || url.fragment().is_some() {
        return Err(AppError::bad_request(
            "yardi_api_base_url_invalid",
            "Yardi API base URL must not contain a query string or fragment",
        ));
    }

    Ok(value.to_string())
}

fn validate_yardi_token_url(value: &str) -> Result<String, AppError> {
    validate_yardi_url(value, "yardi_token_url_invalid", "Yardi token URL")?;
    Ok(value.to_string())
}

fn validate_yardi_url(
    value: &str,
    reason: &'static str,
    field_name: &'static str,
) -> Result<Url, AppError> {
    let url = Url::parse(value).map_err(|_| {
        AppError::bad_request(reason, format!("{field_name} must be a valid absolute URL"))
    })?;

    match url.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(AppError::bad_request(
                reason,
                format!("{field_name} must use http or https"),
            ));
        }
    }

    if url.host_str().is_none() {
        return Err(AppError::bad_request(
            reason,
            format!("{field_name} must include a host"),
        ));
    }

    Ok(url)
}
