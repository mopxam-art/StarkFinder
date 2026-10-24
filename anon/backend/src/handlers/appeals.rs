use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::appeal::{
        Appeal, AppealListResponse, AppealStatus, CreateAppealRequest, ResolveAppealRequest,
    },
    services::appeal_service::{APPEAL_SERVICE, AppealError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Appeal(AppealError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<AppealError> for HandlerError {
    fn from(e: AppealError) -> Self {
        HandlerError::Appeal(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Appeal(e) => e.into_response(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct AppealFilterQuery {
    pub status: Option<AppealStatus>,
}

/// Submit a moderation appeal
#[utoipa::path(
    post,
    path = "/appeals",
    tag = "appeals",
    security(("bearer_auth" = [])),
    request_body = CreateAppealRequest,
    responses(
        (status = 201, description = "Appeal created successfully", body = Appeal),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 409, description = "Conflict - open appeal exists", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn create_appeal(
    AuthUser { wallet }: AuthUser,
    Json(req): Json<CreateAppealRequest>,
) -> Result<(StatusCode, Json<Appeal>), HandlerError> {
    let appeal = APPEAL_SERVICE.create_appeal(&wallet, req)?;
    Ok((StatusCode::CREATED, Json(appeal)))
}

/// Retrieve an appeal by unique ID
#[utoipa::path(
    get,
    path = "/appeals/{id}",
    tag = "appeals",
    params(
        ("id" = String, Path, description = "Appeal UUID")
    ),
    responses(
        (status = 200, description = "Appeal details", body = Appeal),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Appeal not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_appeal_by_id(
    Path(id_str): Path<String>,
) -> Result<Json<Appeal>, HandlerError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid appeal id format")))?;

    let appeal = APPEAL_SERVICE.get_appeal(id)?;
    Ok(Json(appeal))
}

/// List all appeals with optional status filter (Admin)
#[utoipa::path(
    get,
    path = "/admin/appeals",
    tag = "appeals",
    params(
        ("status" = Option<AppealStatus>, Query, description = "Filter by appeal status")
    ),
    responses(
        (status = 200, description = "List of appeals", body = AppealListResponse)
    )
)]
pub async fn list_admin_appeals(
    Query(q): Query<AppealFilterQuery>,
) -> Result<Json<AppealListResponse>, HandlerError> {
    let resp = APPEAL_SERVICE.list_appeals(q.status)?;
    Ok(Json(resp))
}

/// Resolve an appeal with acceptance or rejection note (Admin)
#[utoipa::path(
    put,
    path = "/admin/appeals/{id}",
    tag = "appeals",
    params(
        ("id" = String, Path, description = "Appeal UUID")
    ),
    request_body = ResolveAppealRequest,
    responses(
        (status = 200, description = "Appeal resolved successfully", body = Appeal),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Appeal not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn resolve_admin_appeal(
    Path(id_str): Path<String>,
    Json(req): Json<ResolveAppealRequest>,
) -> Result<Json<Appeal>, HandlerError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid appeal id format")))?;

    let appeal = APPEAL_SERVICE.resolve_appeal(id, req)?;
    Ok(Json(appeal))
}
