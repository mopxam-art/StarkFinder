use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Lifecycle status of a review post
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum PostStatus {
    #[default]
    Published,
    NeedsReview,
    Blocked,
    Deleted,
}

/// A published or archived company review post
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewPost {
    pub id: Uuid,
    pub company_slug: String,
    pub author_wallet: String,
    pub text: String,
    pub tags: Vec<String>,
    pub rating: u8,
    pub status: PostStatus,
    pub created_at: DateTime<Utc>,
}

/// Paginated company-scoped review posts feed response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PostFeedResponse {
    pub items: Vec<ReviewPost>,
    pub total: usize,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
