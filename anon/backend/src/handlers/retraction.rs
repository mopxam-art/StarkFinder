use std::str::FromStr;

use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::retraction::{
        AdminRetractionAuditResponse, CompanyFeedResponse, RetractableReview,
        RetractReviewRequest, RetractReviewResponse,
    },
    services::retraction_service::{RETRACTION_SERVICE, RetractionError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Retraction(RetractionError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<RetractionError> for HandlerError {
    fn from(e: RetractionError) -> Self {
        HandlerError::Retraction(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Retraction(e) => e.into_response(),
        }
    }
}

/// Fetch a single review by ID with 410 Gone lifecycle semantics
#[utoipa::path(
    get,
    path = "/posts/{id}",
    tag = "retraction",
    params(
        ("id" = String, Path, description = "Review UUID")
    ),
    responses(
        (status = 200, description = "Review found and active", body = RetractableReview),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Review not found", body = crate::libs::error::ErrorBody),
        (status = 410, description = "Review has been retracted", body = crate::libs::error::GoneErrorBody)
    )
)]
pub async fn get_post(
    Path(id_str): Path<String>,
) -> Result<Json<RetractableReview>, HandlerError> {
    let review_id = Uuid::from_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid review id format")))?;

    let review = RETRACTION_SERVICE.get_review(review_id)?;
    Ok(Json(review))
}

/// Retract a review (soft delete with 410 Gone semantics)
#[utoipa::path(
    delete,
    path = "/posts/{id}",
    tag = "retraction",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Review UUID")
    ),
    request_body(content = Option<RetractReviewRequest>),
    responses(
        (status = 200, description = "Review retracted successfully", body = RetractReviewResponse),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Missing or invalid token", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Only review author can retract", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Review not found", body = crate::libs::error::ErrorBody),
        (status = 410, description = "Review already retracted", body = crate::libs::error::GoneErrorBody)
    )
)]
pub async fn delete_post(
    AuthUser { wallet }: AuthUser,
    Path(id_str): Path<String>,
    body: Option<Json<RetractReviewRequest>>,
) -> Result<(StatusCode, Json<RetractReviewResponse>), HandlerError> {
    let review_id = Uuid::from_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid review id format")))?;

    let reason = body.and_then(|Json(b)| b.reason);
    let audit = RETRACTION_SERVICE.retract_review(review_id, &wallet, reason)?;

    Ok((
        StatusCode::OK,
        Json(RetractReviewResponse {
            message: "Review retracted successfully".to_string(),
            review_id: audit.review_id,
            retracted_at: audit.retracted_at,
        }),
    ))
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct AdminRetractionsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Admin audit log endpoint for retracted reviews
#[utoipa::path(
    get,
    path = "/admin/retractions",
    tag = "retraction",
    security(("bearer_auth" = [])),
    params(
        AdminRetractionsQuery
    ),
    responses(
        (status = 200, description = "Admin audit log of retractions", body = AdminRetractionAuditResponse),
        (status = 401, description = "Missing or invalid token", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Admin privileges required", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn admin_retractions(
    AuthUser { wallet }: AuthUser,
    Query(params): Query<AdminRetractionsQuery>,
) -> Result<Json<AdminRetractionAuditResponse>, HandlerError> {
    let resp = RETRACTION_SERVICE.admin_list_retracted(&wallet, params.limit, params.offset)?;
    Ok(Json(resp))
}

#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct FeedQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Public feed of reviews for a company, strictly excluding retracted reviews
#[utoipa::path(
    get,
    path = "/feed/{slug}",
    tag = "retraction",
    params(
        ("slug" = String, Path, description = "Company slug"),
        FeedQuery
    ),
    responses(
        (status = 200, description = "Company reviews feed (excluding retracted)", body = CompanyFeedResponse)
    )
)]
pub async fn company_feed(
    Path(slug): Path<String>,
    Query(params): Query<FeedQuery>,
) -> Json<CompanyFeedResponse> {
    let resp = RETRACTION_SERVICE.list_public_feed(&slug, params.limit, params.offset);
    Json(resp)
}
