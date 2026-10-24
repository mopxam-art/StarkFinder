use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Review entity supporting soft retraction and 410 Gone lifecycle semantics
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RetractableReview {
    pub id: Uuid,
    pub company_slug: String,
    pub author_wallet: String,
    pub title: String,
    pub content: String,
    pub rating: u8,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub deleted_reason: Option<String>,
}

impl RetractableReview {
    /// Returns true if this review has been soft-deleted / retracted
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// Request body for retracting a published review
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct RetractReviewRequest {
    pub reason: Option<String>,
}

/// Immutable historical audit entry for review retractions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RetractionAudit {
    pub id: Uuid,
    pub review_id: Uuid,
    pub retracted_by_wallet: String,
    pub retracted_at: DateTime<Utc>,
    pub reason: Option<String>,
}

/// Response returned when a review is successfully retracted
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RetractReviewResponse {
    pub message: String,
    pub review_id: Uuid,
    pub retracted_at: DateTime<Utc>,
}

/// Public feed of reviews for a company, strictly excluding retracted reviews
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyFeedResponse {
    pub company_slug: String,
    pub total_reviews: usize,
    pub reviews: Vec<RetractableReview>,
}

/// Audit log entry exposed to administrators with anonymized author information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AdminRetractionAuditItem {
    pub id: Uuid,
    pub review_id: Uuid,
    pub anonymized_author: String,
    pub retracted_at: DateTime<Utc>,
    pub reason: Option<String>,
}

/// Response returned when administrators inspect retracted reviews
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AdminRetractionAuditResponse {
    pub total: usize,
    pub audits: Vec<AdminRetractionAuditItem>,
}
