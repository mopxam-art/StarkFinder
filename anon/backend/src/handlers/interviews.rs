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
    models::interview::{CreateInterviewRequest, InterviewListResponse, InterviewReview},
    services::interview_service::{INTERVIEW_SERVICE, InterviewError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Interview(InterviewError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<InterviewError> for HandlerError {
    fn from(e: InterviewError) -> Self {
        HandlerError::Interview(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Interview(e) => e.into_response(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct InterviewQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

/// Create a structured interview review
#[utoipa::path(
    post,
    path = "/interviews",
    tag = "interviews",
    security(("bearer_auth" = [])),
    request_body = CreateInterviewRequest,
    responses(
        (status = 201, description = "Interview review created successfully", body = InterviewReview),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn create_interview(
    AuthUser { wallet }: AuthUser,
    Json(req): Json<CreateInterviewRequest>,
) -> Result<(StatusCode, Json<InterviewReview>), HandlerError> {
    let review = INTERVIEW_SERVICE.create_interview(&wallet, req)?;
    Ok((StatusCode::CREATED, Json(review)))
}

/// Retrieve an interview review by unique ID
#[utoipa::path(
    get,
    path = "/interviews/{id}",
    tag = "interviews",
    params(
        ("id" = String, Path, description = "Interview review UUID")
    ),
    responses(
        (status = 200, description = "Interview review details", body = InterviewReview),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Interview review not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_interview_by_id(
    Path(id_str): Path<String>,
) -> Result<Json<InterviewReview>, HandlerError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid interview id format")))?;

    let review = INTERVIEW_SERVICE.get_interview(id)?;
    Ok(Json(review))
}

/// List interview reviews for a company with pagination
#[utoipa::path(
    get,
    path = "/companies/{slug}/interviews",
    tag = "interviews",
    params(
        ("slug" = String, Path, description = "Company slug"),
        ("limit" = Option<usize>, Query, description = "Number of items to return"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor (UUID of last item)")
    ),
    responses(
        (status = 200, description = "Paginated list of interview reviews", body = InterviewListResponse),
        (status = 400, description = "Invalid cursor format", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn list_company_interviews(
    Path(slug): Path<String>,
    Query(q): Query<InterviewQuery>,
) -> Result<Json<InterviewListResponse>, HandlerError> {
    let resp = INTERVIEW_SERVICE.list_company_interviews(&slug, q.limit, q.cursor)?;
    Ok(Json(resp))
}
