use std::str::FromStr;

use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::company_response::{CompanyResponse, CompanyResponseAudit, CreateOrUpdateResponseDto},
    services::company_response_service::{COMPANY_RESPONSE_SERVICE, ResponseError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Service(ResponseError),
}

impl From<ApiError> for HandlerError {
    fn from(err: ApiError) -> Self {
        HandlerError::Api(err)
    }
}

impl From<ResponseError> for HandlerError {
    fn from(err: ResponseError) -> Self {
        HandlerError::Service(err)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Service(e) => e.into_response(),
        }
    }
}

/// Publish or update an official company response for a review
#[utoipa::path(
    post,
    path = "/posts/{id}/response",
    tag = "company_responses",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Review UUID")
    ),
    request_body = CreateOrUpdateResponseDto,
    responses(
        (status = 200, description = "Official response published or updated successfully", body = CompanyResponse),
        (status = 400, description = "Bad request (validation error or invalid UUID)", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized (missing or invalid token)", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden (not a verified company owner)", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Review not found or company not associated", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn post_or_update_response(
    AuthUser { wallet }: AuthUser,
    Path(id_str): Path<String>,
    Json(dto): Json<CreateOrUpdateResponseDto>,
) -> Result<(StatusCode, Json<CompanyResponse>), HandlerError> {
    let review_id = Uuid::from_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid review id format")))?;

    let response = COMPANY_RESPONSE_SERVICE.post_or_update_response_with_slug(
        &wallet,
        review_id,
        &dto.text,
        dto.company_slug.as_deref(),
    )?;

    Ok((StatusCode::OK, Json(response)))
}

/// Retrieve the official company response for a review
#[utoipa::path(
    get,
    path = "/posts/{id}/response",
    tag = "company_responses",
    params(
        ("id" = String, Path, description = "Review UUID")
    ),
    responses(
        (status = 200, description = "Official response retrieved successfully", body = CompanyResponse),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Official response not found for review", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_response(
    Path(id_str): Path<String>,
) -> Result<Json<CompanyResponse>, HandlerError> {
    let review_id = Uuid::from_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid review id format")))?;

    let response = COMPANY_RESPONSE_SERVICE
        .get_response_for_review(review_id)
        .ok_or_else(|| {
            HandlerError::Service(ResponseError::NotFound(
                "official response not found for review".to_string(),
            ))
        })?;

    Ok(Json(response))
}

/// Retrieve the edit audit history for an official company response
#[utoipa::path(
    get,
    path = "/posts/{id}/response/audit",
    tag = "company_responses",
    params(
        ("id" = String, Path, description = "Review UUID")
    ),
    responses(
        (status = 200, description = "Official response audit history retrieved successfully", body = Vec<CompanyResponseAudit>),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Official response not found for review", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_response_audit(
    Path(id_str): Path<String>,
) -> Result<Json<Vec<CompanyResponseAudit>>, HandlerError> {
    let review_id = Uuid::from_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid review id format")))?;

    let history = COMPANY_RESPONSE_SERVICE.get_response_audit_for_review(review_id)?;

    Ok(Json(history))
}
