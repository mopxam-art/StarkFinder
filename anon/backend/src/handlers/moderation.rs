use axum::{
    Json,
    extract::{Path, Query},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::moderation::{
        AuditLogListResponse, UpdateReviewStatusRequest, UpdateReviewStatusResponse,
    },
    services::moderation_service::{MODERATION_SERVICE, ModerationError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Moderation(ModerationError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<ModerationError> for HandlerError {
    fn from(e: ModerationError) -> Self {
        HandlerError::Moderation(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Moderation(e) => e.into_response(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct AuditQuery {
    pub review_id: Option<String>,
}

/// Transition review moderation status (Admin-only)
#[utoipa::path(
    put,
    path = "/admin/reviews/{id}/status",
    tag = "moderation",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Review UUID")
    ),
    request_body = UpdateReviewStatusRequest,
    responses(
        (status = 200, description = "Review status transitioned successfully", body = UpdateReviewStatusResponse),
        (status = 400, description = "Invalid status transition or validation error", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden - requires admin privileges", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn update_review_status(
    AuthUser { wallet }: AuthUser,
    Path(id_str): Path<String>,
    Json(req): Json<UpdateReviewStatusRequest>,
) -> Result<Json<UpdateReviewStatusResponse>, HandlerError> {
    let review_id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid review id format")))?;

    let resp = MODERATION_SERVICE.transition_status(&wallet, review_id, req)?;
    Ok(Json(resp))
}

/// Retrieve review moderation audit trail entries (Admin-only)
#[utoipa::path(
    get,
    path = "/admin/reviews/audit",
    tag = "moderation",
    security(("bearer_auth" = [])),
    params(
        ("review_id" = Option<String>, Query, description = "Filter audit log by specific review UUID")
    ),
    responses(
        (status = 200, description = "Chronological audit trail", body = AuditLogListResponse),
        (status = 400, description = "Invalid review_id format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden - requires admin privileges", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_review_audit_log(
    AuthUser { wallet }: AuthUser,
    Query(q): Query<AuditQuery>,
) -> Result<Json<AuditLogListResponse>, HandlerError> {
    let review_id_opt = match q.review_id {
        Some(s) if !s.trim().is_empty() => Some(
            Uuid::parse_str(&s)
                .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid review_id format")))?,
        ),
        _ => None,
    };

    let resp = MODERATION_SERVICE.get_audit_log(&wallet, review_id_opt)?;
    Ok(Json(resp))
}
