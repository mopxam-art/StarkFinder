use std::str::FromStr;

use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::report::{
        AdminReportsQuery, AdminReportsResponse, CreateReportRequest, CreateReportResponse,
        ReportStatus, ResolveReportRequest, ReviewReport,
    },
    services::report_service::{REPORT_SERVICE, ReportError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Report(ReportError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<ReportError> for HandlerError {
    fn from(e: ReportError) -> Self {
        HandlerError::Report(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Report(e) => e.into_response(),
        }
    }
}

/// Submit a moderation report against a review
#[utoipa::path(
    post,
    path = "/reports",
    tag = "moderation",
    security(("bearer_auth" = [])),
    request_body = CreateReportRequest,
    responses(
        (status = 201, description = "Report created successfully", body = CreateReportResponse),
        (status = 400, description = "Bad request (invalid input / validation error)", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized (missing or invalid token)", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Review not found", body = crate::libs::error::ErrorBody),
        (status = 409, description = "Duplicate active report on this review", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn post_report(
    AuthUser { wallet }: AuthUser,
    Json(req): Json<CreateReportRequest>,
) -> Result<(StatusCode, Json<CreateReportResponse>), HandlerError> {
    let resp = REPORT_SERVICE.create_report(req, &wallet)?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// Inspect the moderation review queue (Admin only)
#[utoipa::path(
    get,
    path = "/admin/reports",
    tag = "moderation",
    security(("bearer_auth" = [])),
    params(
        AdminReportsQuery
    ),
    responses(
        (status = 200, description = "Admin queue of review moderation reports", body = AdminReportsResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Admin privileges required", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_admin_reports(
    AuthUser { wallet }: AuthUser,
    Query(query): Query<AdminReportsQuery>,
) -> Result<Json<AdminReportsResponse>, HandlerError> {
    if !REPORT_SERVICE.is_admin(&wallet) {
        return Err(HandlerError::Report(ReportError::Forbidden(
            "admin privileges required".to_string(),
        )));
    }

    let status_filter = match query.status.as_deref() {
        Some("open") => Some(ReportStatus::Open),
        Some("in_review") => Some(ReportStatus::InReview),
        Some("resolved_dismissed") => Some(ReportStatus::ResolvedDismissed),
        Some("resolved_removed") => Some(ReportStatus::ResolvedRemoved),
        _ => None,
    };

    let resp = REPORT_SERVICE.list_reports(
        status_filter,
        query.limit,
        query.cursor.as_deref(),
    );

    Ok(Json(resp))
}

/// Resolve a moderation report (Admin only)
#[utoipa::path(
    put,
    path = "/admin/reports/{id}",
    tag = "moderation",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Report UUID")
    ),
    request_body = ResolveReportRequest,
    responses(
        (status = 200, description = "Report resolved successfully", body = ReviewReport),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Admin privileges required", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Report not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn resolve_admin_report(
    AuthUser { wallet }: AuthUser,
    Path(id_str): Path<String>,
    Json(req): Json<ResolveReportRequest>,
) -> Result<Json<ReviewReport>, HandlerError> {
    if !REPORT_SERVICE.is_admin(&wallet) {
        return Err(HandlerError::Report(ReportError::Forbidden(
            "admin privileges required".to_string(),
        )));
    }

    let report_id = Uuid::from_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid report id format")))?;

    let resolved = REPORT_SERVICE.resolve_report(report_id, &wallet, req)?;

    Ok(Json(resolved))
}
