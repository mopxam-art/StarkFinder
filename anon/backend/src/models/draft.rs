use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Review draft prior to publication
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewDraft {
    pub id: Uuid,
    pub author_wallet: String,
    pub company_slug: String,
    pub title: String,
    pub content: String,
    pub rating: u8,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Immutable historical audit entry for time-windowed review revisions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewAuditEntry {
    pub id: Uuid,
    pub review_id: Uuid,
    pub editor_wallet: String,
    pub previous_content: String,
    pub new_content: String,
    pub edit_reason: Option<String>,
    pub edited_at: DateTime<Utc>,
}

/// Live published review
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PublishedReview {
    pub id: Uuid,
    pub author_wallet: String,
    pub company_slug: String,
    pub title: String,
    pub content: String,
    pub rating: u8,
    pub tags: Vec<String>,
    pub published_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_edited: bool,
}

/// DTO for creating a review draft
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateDraftDto {
    pub company_slug: String,
    pub title: String,
    pub content: String,
    pub rating: u8,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// DTO for updating an existing review draft
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct UpdateDraftDto {
    pub company_slug: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub rating: Option<u8>,
    pub tags: Option<Vec<String>>,
}

/// DTO for editing a published review within the edit window
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct EditPublishedReviewDto {
    pub content: String,
    pub edit_reason: Option<String>,
}

/// Response returned after creating/updating a draft
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DraftResponse {
    pub draft: ReviewDraft,
    pub message: String,
}

/// Response returned after deleting a draft
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DeleteDraftResponse {
    pub id: Uuid,
    pub message: String,
}

/// Response returned after publishing a draft
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PublishResponse {
    pub published_review: PublishedReview,
    pub message: String,
}

/// Response returned after editing a published review
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EditPublishedReviewResponse {
    pub published_review: PublishedReview,
    pub audit_entry: ReviewAuditEntry,
    pub message: String,
}

/// Response containing review audit history
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AuditHistoryResponse {
    pub review_id: Uuid,
    pub entries: Vec<ReviewAuditEntry>,
}
