use axum::{
    Json,
    extract::{Path, Query},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    models::post::{PostFeedResponse, PostStatus, ReviewPost},
    services::post_service::{POST_SERVICE, PostError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Post(PostError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<PostError> for HandlerError {
    fn from(e: PostError) -> Self {
        HandlerError::Post(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Post(e) => e.into_response(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct CompanyPostsQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    pub tag: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub status: Option<PostStatus>,
}

/// Fetch a single review post by ID (returns 410 Gone if deleted)
#[utoipa::path(
    get,
    path = "/posts/{id}",
    tag = "posts",
    params(
        ("id" = String, Path, description = "Review post UUID")
    ),
    responses(
        (status = 200, description = "Review post details", body = ReviewPost),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Review post not found", body = crate::libs::error::ErrorBody),
        (status = 410, description = "Review post has been soft-deleted", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_post_by_id(
    Path(id_str): Path<String>,
) -> Result<Json<ReviewPost>, HandlerError> {
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid review post id format")))?;

    let post = POST_SERVICE.get_post(id)?;
    Ok(Json(post))
}

/// List review posts for a company with cursor pagination and filtering
#[utoipa::path(
    get,
    path = "/companies/{slug}/posts",
    tag = "posts",
    params(
        ("slug" = String, Path, description = "Company slug"),
        ("limit" = Option<usize>, Query, description = "Items per page (max 50, default 10)"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor (UUID of last item)"),
        ("tag" = Option<String>, Query, description = "Filter by specific tag"),
        ("since" = Option<String>, Query, description = "Filter by minimum created_at timestamp"),
        ("until" = Option<String>, Query, description = "Filter by maximum created_at timestamp"),
        ("status" = Option<PostStatus>, Query, description = "Filter by status (default: 'published')")
    ),
    responses(
        (status = 200, description = "Company posts feed", body = PostFeedResponse),
        (status = 400, description = "Invalid query parameter or cursor format", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn list_company_posts(
    Path(slug): Path<String>,
    Query(q): Query<CompanyPostsQuery>,
) -> Result<Json<PostFeedResponse>, HandlerError> {
    let resp = POST_SERVICE.list_company_posts(
        &slug, q.limit, q.cursor, q.tag, q.since, q.until, q.status,
    )?;
    Ok(Json(resp))
}
