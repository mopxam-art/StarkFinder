use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Status of a company ownership claim
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    #[serde(alias = "Pending", alias = "pending")]
    #[default]
    Pending,
    #[serde(alias = "Approved", alias = "approved")]
    Approved,
    #[serde(alias = "Rejected", alias = "rejected")]
    Rejected,
}

/// Stored company claim record with audit of administrator decision
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyClaim {
    pub id: Uuid,
    pub company_slug: String,
    pub claimant_wallet: String,
    pub proof_uri: Option<String>,
    pub note: Option<String>,
    pub status: ClaimStatus,
    pub decision_note: Option<String>,
    pub decided_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request payload submitted by a claimant to initiate an ownership claim
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct SubmitClaimRequest {
    pub proof_uri: Option<String>,
    pub note: Option<String>,
}

/// Request payload submitted by an administrator to approve or reject a claim
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct AdminClaimDecisionRequest {
    pub action: String,
    pub decision_note: Option<String>,
}

/// Public claim status response for a company
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyClaimStatusResponse {
    pub slug: String,
    pub is_claimed: bool,
    pub pending_count: usize,
    pub owners_count: usize,
}
