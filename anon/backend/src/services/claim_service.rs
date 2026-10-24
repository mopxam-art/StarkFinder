use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, RwLock};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use uuid::Uuid;

use crate::{
    libs::error::ErrorBody,
    models::claim::{
        AdminClaimDecisionRequest, ClaimStatus, CompanyClaim, CompanyClaimStatusResponse,
        SubmitClaimRequest,
    },
};

pub const MAX_NOTE_LENGTH: usize = 1000;
pub const MAX_PROOF_URI_LENGTH: usize = 500;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ClaimError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Internal: {0}")]
    Internal(String),
}

impl IntoResponse for ClaimError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ClaimError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            ClaimError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            ClaimError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            ClaimError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ClaimError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            ClaimError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

/// Canonicalize Starknet wallet address
pub fn canonical_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim();
    crate::libs::wallet::normalize_and_validate(trimmed)
        .unwrap_or_else(|_| trimmed.to_lowercase())
}

/// Canonicalize company slug
pub fn canonical_slug(slug: &str) -> String {
    slug.trim().to_lowercase()
}

/// Thread-safe in-memory store for company ownership claims and verified owner linking
pub struct ClaimService {
    claims: RwLock<HashMap<Uuid, CompanyClaim>>,
    company_owners: RwLock<HashMap<String, HashSet<String>>>,
    admin_wallets: RwLock<HashSet<String>>,
}

impl Default for ClaimService {
    fn default() -> Self {
        Self::new()
    }
}

pub static CLAIM_SERVICE: LazyLock<ClaimService> = LazyLock::new(ClaimService::default);

impl ClaimService {
    pub fn new() -> Self {
        Self {
            claims: RwLock::new(HashMap::new()),
            company_owners: RwLock::new(HashMap::new()),
            admin_wallets: RwLock::new(HashSet::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut cl = self.claims.write().unwrap();
        cl.clear();
        let mut co = self.company_owners.write().unwrap();
        co.clear();
        let mut adm = self.admin_wallets.write().unwrap();
        adm.clear();
    }

    /// Register an administrator wallet
    pub fn add_admin(&self, wallet: &str) {
        let norm = canonical_wallet(wallet);
        let mut adm = self.admin_wallets.write().unwrap();
        adm.insert(norm);
    }

    /// Check if wallet has administrative privileges
    pub fn is_admin(&self, wallet: &str) -> bool {
        let norm = canonical_wallet(wallet);
        let adm = self.admin_wallets.read().unwrap();
        adm.contains(&norm)
    }

    /// Add a verified company owner
    pub fn add_company_owner(&self, company_slug: &str, wallet: &str) {
        let slug = canonical_slug(company_slug);
        let norm = canonical_wallet(wallet);
        let mut owners = self.company_owners.write().unwrap();
        owners.entry(slug).or_default().insert(norm);
    }

    /// Check if a wallet is a verified company owner
    pub fn is_company_owner(&self, company_slug: &str, wallet: &str) -> bool {
        let slug = canonical_slug(company_slug);
        let norm = canonical_wallet(wallet);
        let owners = self.company_owners.read().unwrap();
        owners
            .get(&slug)
            .map(|set| set.contains(&norm))
            .unwrap_or(false)
    }

    /// Submit a claim to become a verified company owner
    pub fn submit_claim(
        &self,
        company_slug: &str,
        claimant_wallet: &str,
        req: SubmitClaimRequest,
    ) -> Result<CompanyClaim, ClaimError> {
        let slug = canonical_slug(company_slug);
        if slug.is_empty() {
            return Err(ClaimError::BadRequest("invalid company slug".to_string()));
        }

        // Validate lengths
        if let Some(ref proof) = req.proof_uri {
            if proof.chars().count() > MAX_PROOF_URI_LENGTH {
                return Err(ClaimError::BadRequest(format!(
                    "proof_uri exceeds maximum length of {} characters",
                    MAX_PROOF_URI_LENGTH
                )));
            }
        }
        if let Some(ref note) = req.note {
            if note.chars().count() > MAX_NOTE_LENGTH {
                return Err(ClaimError::BadRequest(format!(
                    "note exceeds maximum length of {} characters",
                    MAX_NOTE_LENGTH
                )));
            }
        }

        let norm_claimant = canonical_wallet(claimant_wallet);

        // Check if already a verified owner
        if self.is_company_owner(&slug, &norm_claimant) {
            return Err(ClaimError::Conflict(
                "wallet is already a verified owner of this company".to_string(),
            ));
        }

        let mut claims = self.claims.write().unwrap();

        // Check duplicate pending claims
        let has_pending = claims.values().any(|c| {
            c.company_slug == slug
                && c.claimant_wallet == norm_claimant
                && c.status == ClaimStatus::Pending
        });

        if has_pending {
            return Err(ClaimError::Conflict("claim already pending".to_string()));
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        let claim = CompanyClaim {
            id,
            company_slug: slug,
            claimant_wallet: norm_claimant,
            proof_uri: req.proof_uri,
            note: req.note,
            status: ClaimStatus::Pending,
            decision_note: None,
            decided_by: None,
            created_at: now,
            updated_at: now,
        };

        claims.insert(id, claim.clone());

        Ok(claim)
    }

    /// Get public claim status for a company
    pub fn get_company_claim_status(&self, company_slug: &str) -> CompanyClaimStatusResponse {
        let slug = canonical_slug(company_slug);

        let owners = self.company_owners.read().unwrap();
        let owners_count = owners.get(&slug).map(|set| set.len()).unwrap_or(0);
        let is_claimed = owners_count > 0;

        let claims = self.claims.read().unwrap();
        let pending_count = claims
            .values()
            .filter(|c| c.company_slug == slug && c.status == ClaimStatus::Pending)
            .count();

        CompanyClaimStatusResponse {
            slug,
            is_claimed,
            pending_count,
            owners_count,
        }
    }

    /// Admin decision on a company claim (approve or reject)
    pub fn admin_decide_claim(
        &self,
        company_slug: &str,
        claim_id: Uuid,
        admin_wallet: &str,
        req: AdminClaimDecisionRequest,
    ) -> Result<CompanyClaim, ClaimError> {
        let norm_admin = canonical_wallet(admin_wallet);
        if !self.is_admin(&norm_admin) {
            return Err(ClaimError::Forbidden(
                "admin privileges required".to_string(),
            ));
        }

        let slug = canonical_slug(company_slug);
        let mut claims = self.claims.write().unwrap();
        let claim = claims
            .get_mut(&claim_id)
            .ok_or_else(|| ClaimError::NotFound("claim not found".to_string()))?;

        if claim.company_slug != slug {
            return Err(ClaimError::NotFound("claim not found for this company".to_string()));
        }

        let action_lower = req.action.trim().to_lowercase();
        let now = Utc::now();

        match action_lower.as_str() {
            "approve" => {
                claim.status = ClaimStatus::Approved;
                claim.decision_note = req.decision_note;
                claim.decided_by = Some(norm_admin);
                claim.updated_at = now;

                // Link wallet as verified company owner
                let mut owners = self.company_owners.write().unwrap();
                owners.entry(slug).or_default().insert(claim.claimant_wallet.clone());
            }
            "reject" => {
                claim.status = ClaimStatus::Rejected;
                claim.decision_note = req.decision_note;
                claim.decided_by = Some(norm_admin);
                claim.updated_at = now;
            }
            _ => {
                return Err(ClaimError::BadRequest(
                    "action must be 'approve' or 'reject'".to_string(),
                ));
            }
        }

        Ok(claim.clone())
    }

    /// Retrieve a claim by ID
    pub fn get_claim(&self, claim_id: Uuid) -> Option<CompanyClaim> {
        let claims = self.claims.read().unwrap();
        claims.get(&claim_id).cloned()
    }
}
