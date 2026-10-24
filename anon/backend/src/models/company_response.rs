use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Official response published by verified company owner for a review
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyResponse {
    pub id: Uuid,
    pub review_id: Uuid,
    pub company_slug: String,
    pub owner_wallet: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Immutable historical audit entry for company response revisions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyResponseAudit {
    pub id: Uuid,
    pub response_id: Uuid,
    pub previous_text: String,
    pub new_text: String,
    pub editor_wallet: String,
    pub edited_at: DateTime<Utc>,
}

/// DTO for creating or updating an official company response
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CreateOrUpdateResponseDto {
    pub text: String,
    #[serde(default)]
    pub company_slug: Option<String>,
}

/// API response payload containing the company response and status message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyResponseRes {
    pub response: CompanyResponse,
    pub message: String,
}

/// API response payload containing audit history for a company response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompanyResponseAuditRes {
    pub response_id: Uuid,
    pub audit_history: Vec<CompanyResponseAudit>,
}
