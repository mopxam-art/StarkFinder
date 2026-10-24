use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Lifecycle status of a zkTLS employment verification attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    #[default]
    Pending,
    Verified,
    Failed,
}

/// Request to start a zkTLS verification session
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct StartVerificationRequest {
    pub company_slug: String,
    pub provider: Option<String>,
}

/// Response returned to initialize the client zkTLS verification session
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StartVerificationResponse {
    pub session_id: Uuid,
    pub company_slug: String,
    pub challenge: String,
    pub expires_at: DateTime<Utc>,
    pub provider_session_url: String,
}

/// Request payload submitting the generated zkTLS proof
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct SubmitProofRequest {
    pub session_id: Uuid,
    pub proof_blob: String,
    pub public_inputs: Option<serde_json::Value>,
}

/// Response returned after proof verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SubmitProofResponse {
    pub session_id: Uuid,
    pub status: VerificationStatus,
    pub attestation_id: Option<Uuid>,
    pub verified_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

/// Current employment verification status for a user and company
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VerificationStatusResponse {
    pub company_slug: String,
    pub status: VerificationStatus,
    pub attestation_id: Option<Uuid>,
    pub verified_at: Option<DateTime<Utc>>,
}
