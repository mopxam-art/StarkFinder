use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Saved bookmark record linking a user wallet to a specific post/review
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Bookmark {
    pub id: Uuid,
    pub user_wallet: String,
    pub post_id: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Paginated list of bookmarks returned for the authenticated user
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BookmarkResponse {
    pub items: Vec<Bookmark>,
    pub total: usize,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// Bookmark status response for checking if a specific post is saved
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BookmarkStatusResponse {
    pub post_id: Uuid,
    pub is_bookmarked: bool,
}
