use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Actor submitting the appeal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum AppealActor {
    #[default]
    Author,
    Owner,
}

/// Lifecycle status of an appeal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum AppealStatus {
    #[default]
    Open,
    Accepted,
    Rejected,
}

/// Appeal record for moderation decisions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Appeal {
    pub id: Uuid,
    pub review_id: Uuid,
    pub actor: AppealActor,
    pub appellant_wallet: String,
    pub reason: String,
    pub status: AppealStatus,
    pub resolution_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Request payload to submit an appeal
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct CreateAppealRequest {
    pub review_id: Uuid,
    pub actor: AppealActor,
    pub reason: String,
}

/// Request payload for admin to resolve an appeal
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct ResolveAppealRequest {
    pub status: AppealStatus,
    pub resolution_note: String,
}

/// List response containing appeals
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AppealListResponse {
    pub items: Vec<Appeal>,
    pub total: usize,
}
