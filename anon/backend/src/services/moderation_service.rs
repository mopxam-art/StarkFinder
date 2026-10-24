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
    models::moderation::{
        AuditLogListResponse, ReviewAuditEntry, ReviewModerationStatus, UpdateReviewStatusRequest,
        UpdateReviewStatusResponse,
    },
};

pub const MAX_REASON_LEN: usize = 1000;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ModerationError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for ModerationError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ModerationError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ModerationError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            ModerationError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            ModerationError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

/// Canonicalize Starknet wallet address
pub fn canonical_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim().to_lowercase();
    crate::libs::wallet::normalize_and_validate(&trimmed).unwrap_or(trimmed)
}

/// Basic XSS and script sanitization helper
pub fn sanitize_text(input: &str) -> String {
    input
        .replace("<script>", "&lt;script&gt;")
        .replace("</script>", "&lt;/script&gt;")
        .replace("<script", "&lt;script")
        .replace("javascript:", "")
        .replace("onload=", "")
        .replace("onerror=", "")
}

/// Verify allowed transitions in the review moderation finite-state machine
pub fn is_valid_transition(from: ReviewModerationStatus, to: ReviewModerationStatus) -> bool {
    if from == to {
        return false;
    }
    if from == ReviewModerationStatus::Deleted {
        return false; // Terminal state
    }
    if to == ReviewModerationStatus::Deleted {
        return true; // Any non-deleted review can be transitioned to deleted
    }

    match from {
        ReviewModerationStatus::Published => to == ReviewModerationStatus::NeedsReview,
        ReviewModerationStatus::NeedsReview => {
            to == ReviewModerationStatus::Published || to == ReviewModerationStatus::Blocked
        }
        ReviewModerationStatus::Blocked => to == ReviewModerationStatus::NeedsReview,
        ReviewModerationStatus::Deleted => false,
    }
}

/// Thread-safe in-memory store for moderation statuses and audit trail
pub struct ModerationService {
    statuses: RwLock<HashMap<Uuid, ReviewModerationStatus>>,
    audit_log: RwLock<Vec<ReviewAuditEntry>>,
    admins: RwLock<HashSet<String>>,
}

impl Default for ModerationService {
    fn default() -> Self {
        Self::new()
    }
}

pub static MODERATION_SERVICE: LazyLock<ModerationService> =
    LazyLock::new(ModerationService::default);

impl ModerationService {
    pub fn new() -> Self {
        Self {
            statuses: RwLock::new(HashMap::new()),
            audit_log: RwLock::new(Vec::new()),
            admins: RwLock::new(HashSet::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut st = self.statuses.write().unwrap();
        st.clear();
        let mut au = self.audit_log.write().unwrap();
        au.clear();
        let mut ad = self.admins.write().unwrap();
        ad.clear();
    }

    /// Register an authorized admin wallet
    pub fn add_admin(&self, wallet: &str) {
        let norm = canonical_wallet(wallet);
        let mut ad = self.admins.write().unwrap();
        ad.insert(norm);
    }

    /// Check if wallet possesses administrator privileges
    pub fn is_admin(&self, wallet: &str) -> bool {
        let norm = canonical_wallet(wallet);
        let ad = self.admins.read().unwrap();
        ad.contains(&norm)
    }

    /// Helper to seed an existing review with a given status
    pub fn seed_review_status(&self, review_id: Uuid, status: ReviewModerationStatus) {
        let mut st = self.statuses.write().unwrap();
        st.insert(review_id, status);
    }

    /// Get current status of a review (defaults to Published if not explicitly set)
    pub fn get_review_status(&self, review_id: Uuid) -> ReviewModerationStatus {
        let st = self.statuses.read().unwrap();
        st.get(&review_id)
            .copied()
            .unwrap_or(ReviewModerationStatus::Published)
    }

    /// Transition a review's moderation status and log to the immutable audit trail
    pub fn transition_status(
        &self,
        admin_wallet: &str,
        review_id: Uuid,
        req: UpdateReviewStatusRequest,
    ) -> Result<UpdateReviewStatusResponse, ModerationError> {
        let norm_admin = canonical_wallet(admin_wallet);
        if !self.is_admin(&norm_admin) {
            return Err(ModerationError::Forbidden(
                "action requires administrator privileges".to_string(),
            ));
        }

        if let Some(ref reason) = req.reason {
            if reason.trim().chars().count() > MAX_REASON_LEN {
                return Err(ModerationError::BadRequest(format!(
                    "reason exceeds maximum length of {} characters",
                    MAX_REASON_LEN
                )));
            }
        }

        let mut st = self.statuses.write().unwrap();
        let current_status = st
            .get(&review_id)
            .copied()
            .unwrap_or(ReviewModerationStatus::Published);

        if current_status == req.status {
            return Err(ModerationError::BadRequest(format!(
                "review is already in '{:?}' status",
                current_status
            )));
        }

        if current_status == ReviewModerationStatus::Deleted {
            return Err(ModerationError::BadRequest(
                "cannot transition from terminal deleted state".to_string(),
            ));
        }

        if !is_valid_transition(current_status, req.status) {
            return Err(ModerationError::BadRequest(format!(
                "invalid status transition from '{:?}' to '{:?}'",
                current_status, req.status
            )));
        }

        let sanitized_reason = req.reason.map(|r| sanitize_text(r.trim()));
        let now = Utc::now();

        // Update status
        st.insert(review_id, req.status);

        // Record audit entry
        let audit_entry = ReviewAuditEntry {
            id: Uuid::new_v4(),
            review_id,
            prev_status: current_status,
            new_status: req.status,
            reason: sanitized_reason,
            admin_wallet: norm_admin,
            at: now,
        };

        let mut au = self.audit_log.write().unwrap();
        au.push(audit_entry);

        Ok(UpdateReviewStatusResponse {
            review_id,
            prev_status: current_status,
            current_status: req.status,
            updated_at: now,
        })
    }

    /// Retrieve the audit trail history
    pub fn get_audit_log(
        &self,
        admin_wallet: &str,
        review_id_opt: Option<Uuid>,
    ) -> Result<AuditLogListResponse, ModerationError> {
        let norm_admin = canonical_wallet(admin_wallet);
        if !self.is_admin(&norm_admin) {
            return Err(ModerationError::Forbidden(
                "viewing audit log requires administrator privileges".to_string(),
            ));
        }

        let au = self.audit_log.read().unwrap();
        let mut entries: Vec<ReviewAuditEntry> = match review_id_opt {
            Some(rid) => au.iter().filter(|e| e.review_id == rid).cloned().collect(),
            None => au.clone(),
        };

        // Chronological ascending order
        entries.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));

        let total = entries.len();
        Ok(AuditLogListResponse {
            items: entries,
            total,
        })
    }
}
