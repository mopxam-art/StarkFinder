use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Status of an asynchronous data export job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    #[default]
    Pending,
    Ready,
    Error,
}

/// A sanitized review record included in the user data export
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExportedReview {
    pub id: Uuid,
    pub company_slug: String,
    pub title: Option<String>,
    pub content: String,
    pub rating: u8,
    pub created_at: DateTime<Utc>,
}

/// Complete exported data package delivered to the user
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExportDataPayload {
    pub user_wallet: String,
    pub exported_at: DateTime<Utc>,
    pub total: usize,
    pub reviews: Vec<ExportedReview>,
}

/// Metadata and lifecycle of a user data export request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExportJob {
    pub id: Uuid,
    pub user_wallet: String,
    pub status: ExportStatus,
    pub download_url: Option<String>,
    pub token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub total_reviews: usize,
    pub created_at: DateTime<Utc>,
}

/// Status response returned to the client polling export completion
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ExportStatusResponse {
    pub job_id: Uuid,
    pub status: ExportStatus,
    pub download_url: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub message: String,
}
