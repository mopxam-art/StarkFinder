use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Categories / reasons for reporting a review
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportReasonCode {
    #[serde(alias = "Spam", alias = "spam")]
    Spam,
    #[serde(alias = "Harassment", alias = "harassment")]
    Harassment,
    #[serde(alias = "HateSpeech", alias = "hate_speech")]
    HateSpeech,
    #[serde(alias = "PiiLeak", alias = "pii_leak")]
    PiiLeak,
    #[serde(alias = "FalseInformation", alias = "false_information")]
    FalseInformation,
    #[serde(alias = "Other", alias = "other")]
    Other,
}

/// Lifecycle status of a review moderation report
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    #[serde(alias = "Open", alias = "open")]
    #[default]
    Open,
    #[serde(alias = "InReview", alias = "in_review")]
    InReview,
    #[serde(alias = "ResolvedDismissed", alias = "resolved_dismissed")]
    ResolvedDismissed,
    #[serde(alias = "ResolvedRemoved", alias = "resolved_removed")]
    ResolvedRemoved,
}

/// User request payload for reporting a review
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateReportRequest {
    pub review_id: Uuid,
    pub reason_code: ReportReasonCode,
    pub note: Option<String>,
}

/// API response payload returned upon successful creation of a report
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateReportResponse {
    pub id: Uuid,
    pub status: ReportStatus,
    pub created_at: DateTime<Utc>,
}

/// Stored moderation report record with privacy-preserving pseudonymized reporter ID
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewReport {
    pub id: Uuid,
    pub review_id: Uuid,
    pub reporter_user_id: String,
    pub reason_code: ReportReasonCode,
    pub note: Option<String>,
    pub status: ReportStatus,
    pub resolution_note: Option<String>,
    pub resolved_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Admin request payload to update status and resolution note of a report
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct ResolveReportRequest {
    pub status: ReportStatus,
    pub resolution_note: Option<String>,
}

/// Paginated admin queue response for review moderation reports
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AdminReportsResponse {
    pub total: usize,
    pub items: Vec<ReviewReport>,
    pub cursor: Option<String>,
}

/// Query parameters for filtering and paginating admin reports queue
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct AdminReportsQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}
