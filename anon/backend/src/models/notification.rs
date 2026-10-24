use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Target kind for subscriptions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FollowKind {
    Company,
    Tag,
}

impl std::fmt::Display for FollowKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FollowKind::Company => write!(f, "company"),
            FollowKind::Tag => write!(f, "tag"),
        }
    }
}

/// Core subscription relationship model
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Follow {
    pub follower_user_id: String,
    pub kind: FollowKind,
    pub target_id: String,
    pub created_at: DateTime<Utc>,
}

/// Supported notification categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    NewReview,
    NewSummary,
    StatusChange,
    Mention,
}

impl std::fmt::Display for NotificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationType::NewReview => write!(f, "new_review"),
            NotificationType::NewSummary => write!(f, "new_summary"),
            NotificationType::StatusChange => write!(f, "status_change"),
            NotificationType::Mention => write!(f, "mention"),
        }
    }
}

/// Privacy-preserving in-app alert model
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: String,
    pub notification_type: NotificationType,
    pub payload: serde_json::Value,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Query parameters for fetching user notifications
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct NotificationsQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub only_unread: Option<bool>,
}

/// Generic cursor-based paginated response container
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub total_count: usize,
    pub unread_count: usize,
    pub has_more: bool,
}

pub type NotificationsListRes = PaginatedResponse<Notification>;

/// Response after successfully following a company or tag
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FollowResponse {
    pub follower_user_id: String,
    pub kind: FollowKind,
    pub target_id: String,
    pub created_at: DateTime<Utc>,
    pub message: String,
}

/// Response after unfollowing a company or tag
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnfollowResponse {
    pub message: String,
    pub kind: FollowKind,
    pub target_id: String,
}

/// Response checking if a user follows a target
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FollowStatusResponse {
    pub is_following: bool,
    pub kind: FollowKind,
    pub target_id: String,
}

/// Response listing followers of a target
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ListFollowersResponse {
    pub kind: FollowKind,
    pub target_id: String,
    pub follower_user_ids: Vec<String>,
    pub total_count: usize,
}

/// Response after marking a single notification as read
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MarkReadResponse {
    pub notification: Notification,
    pub message: String,
}

/// Response after marking all notifications as read
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MarkAllReadResponse {
    pub updated_count: usize,
    pub message: String,
}

/// DTO for system dispatch of in-app notifications
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct DispatchNotificationDto {
    pub target_users: Vec<String>,
    pub notification_type: NotificationType,
    pub payload: serde_json::Value,
}

/// Response after dispatching notifications
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DispatchResponse {
    pub dispatched_count: usize,
    pub message: String,
}
