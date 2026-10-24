use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Moderation status state machine enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReviewModerationStatus {
    #[default]
    Published,
    NeedsReview,
    Blocked,
    Deleted,
}

/// Immutable audit trail record for review status transitions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewAuditEntry {
    pub id: Uuid,
    pub review_id: Uuid,
    pub prev_status: ReviewModerationStatus,
    pub new_status: ReviewModerationStatus,
    pub reason: Option<String>,
    pub admin_wallet: String,
    pub at: DateTime<Utc>,
}

/// Request payload to update review moderation status
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct UpdateReviewStatusRequest {
    pub status: ReviewModerationStatus,
    pub reason: Option<String>,
}

/// Response returned after successfully transitioning review status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdateReviewStatusResponse {
    pub review_id: Uuid,
    pub prev_status: ReviewModerationStatus,
    pub current_status: ReviewModerationStatus,
    pub updated_at: DateTime<Utc>,
}

/// Paginated or filtered list of review moderation audit entries
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuditLogListResponse {
    pub items: Vec<ReviewAuditEntry>,
    pub total: usize,
}
