use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Status lifecycle for a comment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum CommentStatus {
    #[default]
    Active,
    Deleted,
    Moderated,
}

/// Core internal comment model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub post_id: String,
    pub author_wallet: String,
    pub parent_id: Option<String>,
    pub content: String,
    pub depth: usize,
    pub status: CommentStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// DTO for creating a new comment or nested reply
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateCommentDto {
    pub content: String,
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Public representation of a comment in a discussion tree
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct CommentNode {
    pub id: String,
    pub post_id: String,
    pub author_wallet: String,
    pub parent_id: Option<String>,
    pub content: String,
    pub depth: usize,
    pub status: CommentStatus,
    pub reply_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Query parameters for listing comments under a review/post
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct ListCommentsQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

/// Paginated list of threaded comments
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CommentsListRes {
    pub comments: Vec<CommentNode>,
    pub next_cursor: Option<String>,
    pub total_count: usize,
    pub has_more: bool,
}

/// Response returned on comment deletion
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeleteCommentRes {
    pub message: String,
    pub comment_id: String,
    pub status: CommentStatus,
}
