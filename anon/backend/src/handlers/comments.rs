use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::comment::{
        CommentNode, CommentsListRes, CreateCommentDto, DeleteCommentRes, ListCommentsQuery,
    },
    services::comment_service::COMMENT_STORE,
};

/// POST /posts/{id}/comments
/// Create a new comment or threaded reply under a review/post
#[utoipa::path(
    post,
    path = "/posts/{id}/comments",
    tag = "comments",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Post or review UUID")
    ),
    request_body = CreateCommentDto,
    responses(
        (status = 201, description = "Comment created successfully", body = CommentNode),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Post or parent comment not found", body = crate::libs::error::ErrorBody),
        (status = 410, description = "Cannot reply to deleted comment", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn create_comment(
    Path(post_id): Path<String>,
    AuthUser { wallet }: AuthUser,
    Json(dto): Json<CreateCommentDto>,
) -> Result<(StatusCode, Json<CommentNode>), ApiError> {
    let node = COMMENT_STORE.create_comment(&post_id, &wallet, dto)?;
    Ok((StatusCode::CREATED, Json(node)))
}

/// GET /posts/{id}/comments
/// Retrieve threaded discussion comments for a review/post
#[utoipa::path(
    get,
    path = "/posts/{id}/comments",
    tag = "comments",
    params(
        ("id" = String, Path, description = "Post or review UUID"),
        ("cursor" = Option<String>, Query, description = "Cursor for pagination"),
        ("limit" = Option<usize>, Query, description = "Number of comments to return (default 20, max 100)")
    ),
    responses(
        (status = 200, description = "Threaded discussion comments list", body = CommentsListRes),
        (status = 404, description = "Post not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn list_comments(
    Path(post_id): Path<String>,
    Query(query): Query<ListCommentsQuery>,
) -> Result<Json<CommentsListRes>, ApiError> {
    let res = COMMENT_STORE.list_comments(
        &post_id,
        query.cursor.as_deref(),
        query.limit,
    )?;
    Ok(Json(res))
}

/// DELETE /comments/{id}
/// Author or moderator soft delete of a comment
#[utoipa::path(
    delete,
    path = "/comments/{id}",
    tag = "comments",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Comment UUID to soft delete")
    ),
    responses(
        (status = 200, description = "Comment soft deleted", body = DeleteCommentRes),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden: not author or moderator", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Comment not found", body = crate::libs::error::ErrorBody),
        (status = 410, description = "Comment already deleted", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn delete_comment(
    Path(comment_id): Path<String>,
    AuthUser { wallet }: AuthUser,
) -> Result<Json<DeleteCommentRes>, ApiError> {
    let node = COMMENT_STORE.soft_delete_comment(&comment_id, &wallet, false)?;
    Ok(Json(DeleteCommentRes {
        message: "comment soft deleted successfully".to_string(),
        comment_id,
        status: node.status,
    }))
}
