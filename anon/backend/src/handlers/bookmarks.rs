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
    models::bookmark::{Bookmark, BookmarkResponse, BookmarkStatusResponse},
    services::bookmark_service::{BOOKMARK_SERVICE, BookmarkError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Bookmark(BookmarkError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<BookmarkError> for HandlerError {
    fn from(e: BookmarkError) -> Self {
        HandlerError::Bookmark(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Bookmark(e) => e.into_response(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct BookmarkQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

/// Save a review/post to user bookmarks
#[utoipa::path(
    post,
    path = "/posts/{id}/bookmark",
    tag = "bookmarks",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Post UUID")
    ),
    responses(
        (status = 201, description = "Review bookmarked successfully", body = Bookmark),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn add_bookmark(
    AuthUser { wallet }: AuthUser,
    Path(id_str): Path<String>,
) -> Result<(StatusCode, Json<Bookmark>), HandlerError> {
    let post_id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid post id format")))?;

    let bookmark = BOOKMARK_SERVICE.add_bookmark(&wallet, post_id)?;
    Ok((StatusCode::CREATED, Json(bookmark)))
}

/// Remove a review/post from user bookmarks
#[utoipa::path(
    delete,
    path = "/posts/{id}/bookmark",
    tag = "bookmarks",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Post UUID")
    ),
    responses(
        (status = 204, description = "Bookmark removed successfully"),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn remove_bookmark(
    AuthUser { wallet }: AuthUser,
    Path(id_str): Path<String>,
) -> Result<StatusCode, HandlerError> {
    let post_id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid post id format")))?;

    BOOKMARK_SERVICE.remove_bookmark(&wallet, post_id)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Retrieve saved bookmarks for the authenticated user
#[utoipa::path(
    get,
    path = "/me/bookmarks",
    tag = "bookmarks",
    security(("bearer_auth" = [])),
    params(
        ("limit" = Option<usize>, Query, description = "Number of items to return"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor (UUID of last item)")
    ),
    responses(
        (status = 200, description = "List of user bookmarks", body = BookmarkResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn list_my_bookmarks(
    AuthUser { wallet }: AuthUser,
    Query(q): Query<BookmarkQuery>,
) -> Result<Json<BookmarkResponse>, HandlerError> {
    let resp = BOOKMARK_SERVICE.list_bookmarks(&wallet, q.limit, q.cursor)?;
    Ok(Json(resp))
}

/// Check if a specific review is bookmarked by the authenticated user
#[utoipa::path(
    get,
    path = "/posts/{id}/bookmark",
    tag = "bookmarks",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Post UUID")
    ),
    responses(
        (status = 200, description = "Bookmark status of post", body = BookmarkStatusResponse),
        (status = 400, description = "Invalid UUID format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_bookmark_status(
    AuthUser { wallet }: AuthUser,
    Path(id_str): Path<String>,
) -> Result<Json<BookmarkStatusResponse>, HandlerError> {
    let post_id = Uuid::parse_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid post id format")))?;

    let status = BOOKMARK_SERVICE.get_bookmark_status(&wallet, post_id);
    Ok(Json(status))
}
