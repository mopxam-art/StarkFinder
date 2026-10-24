use axum::{
    Json,
    extract::Path,
    http::header,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::export::{ExportDataPayload, ExportStatusResponse},
    services::export_service::{EXPORT_SERVICE, ExportError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Export(ExportError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<ExportError> for HandlerError {
    fn from(e: ExportError) -> Self {
        HandlerError::Export(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Export(e) => e.into_response(),
        }
    }
}

/// Request an asynchronous data export of all user-authored reviews
#[utoipa::path(
    post,
    path = "/me/export",
    tag = "export",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Export job initiated / ready", body = ExportStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 429, description = "Rate limit exceeded (1 export per 24 hours)", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn request_my_export(
    AuthUser { wallet }: AuthUser,
) -> Result<Json<ExportStatusResponse>, HandlerError> {
    let resp = EXPORT_SERVICE.request_export(&wallet)?;
    Ok(Json(resp))
}

/// Check the status of a requested data export job
#[utoipa::path(
    get,
    path = "/me/export/{id}/status",
    tag = "export",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Export job UUID")
    ),
    responses(
        (status = 200, description = "Export job status", body = ExportStatusResponse),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden (not your job)", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Job not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_my_export_status(
    AuthUser { wallet }: AuthUser,
    Path(id_str): Path<String>,
) -> Result<Json<ExportStatusResponse>, HandlerError> {
    let job_id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid job id format")))?;

    let status = EXPORT_SERVICE.get_export_status(&wallet, job_id)?;
    Ok(Json(status))
}

/// Download the exported JSON package by temporary signed token
#[utoipa::path(
    get,
    path = "/exports/download/{token}",
    tag = "export",
    params(
        ("token" = String, Path, description = "One-time download token")
    ),
    responses(
        (status = 200, description = "Export data payload JSON download", body = ExportDataPayload),
        (status = 404, description = "Token not found or invalid", body = crate::libs::error::ErrorBody),
        (status = 410, description = "Download link has expired", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn download_export_package(
    Path(token): Path<String>,
) -> Result<impl IntoResponse, HandlerError> {
    let data = EXPORT_SERVICE.download_export(&token)?;
    let headers = [(
        header::CONTENT_DISPOSITION,
        "attachment; filename=\"anondoor_reviews_export.json\"",
    )];
    Ok((headers, Json(data)))
}
