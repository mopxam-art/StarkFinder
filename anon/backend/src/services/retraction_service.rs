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
    libs::error::{ErrorBody, GoneErrorBody},
    models::retraction::{
        AdminRetractionAuditItem, AdminRetractionAuditResponse, CompanyFeedResponse,
        RetractableReview, RetractionAudit,
    },
};

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RetractionError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Gone: {0}")]
    Gone(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Internal: {0}")]
    Internal(String),
}

impl IntoResponse for RetractionError {
    fn into_response(self) -> Response {
        match self {
            RetractionError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                Json(ErrorBody { error: msg }),
            )
                .into_response(),
            RetractionError::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                Json(ErrorBody { error: msg }),
            )
                .into_response(),
            RetractionError::Gone(msg) => (
                StatusCode::GONE,
                Json(GoneErrorBody {
                    error: msg,
                    code: 410,
                }),
            )
                .into_response(),
            RetractionError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody { error: msg }),
            )
                .into_response(),
            RetractionError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody { error: msg }),
            )
                .into_response(),
            RetractionError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody { error: msg }),
            )
                .into_response(),
        }
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

/// Anonymize wallet address for audit compliance (e.g. 0x01...abcd or 0x1...anon)
pub fn anonymize_wallet(wallet: &str) -> String {
    let norm = canonical_wallet(wallet);
    if norm.len() > 8 {
        format!("{}...{}", &norm[..4], &norm[norm.len() - 4..])
    } else {
        format!("{}...anon", norm)
    }
}

/// In-memory store managing review retractions, soft-delete state, and audit trails
pub struct RetractionService {
    reviews: RwLock<HashMap<Uuid, RetractableReview>>,
    audits: RwLock<Vec<RetractionAudit>>,
    admin_wallets: RwLock<HashSet<String>>,
}

impl Default for RetractionService {
    fn default() -> Self {
        Self::new()
    }
}

pub static RETRACTION_SERVICE: LazyLock<RetractionService> =
    LazyLock::new(RetractionService::default);

impl RetractionService {
    pub fn new() -> Self {
        Self {
            reviews: RwLock::new(HashMap::new()),
            audits: RwLock::new(Vec::new()),
            admin_wallets: RwLock::new(HashSet::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut r = self.reviews.write().unwrap();
        r.clear();
        let mut a = self.audits.write().unwrap();
        a.clear();
        let mut adm = self.admin_wallets.write().unwrap();
        adm.clear();
    }

    /// Register an administrator wallet
    pub fn add_admin(&self, wallet: &str) {
        let norm = canonical_wallet(wallet);
        let mut adm = self.admin_wallets.write().unwrap();
        adm.insert(norm);
    }

    /// Check if wallet has administrative rights
    pub fn is_admin(&self, wallet: &str) -> bool {
        let norm = canonical_wallet(wallet);
        let adm = self.admin_wallets.read().unwrap();
        adm.contains(&norm)
    }

    /// Create or register a review in the store
    pub fn register_review(&self, review: RetractableReview) {
        let mut r = self.reviews.write().unwrap();
        r.insert(review.id, review);
    }

    /// Retract a review (soft delete with audit record and 410 Gone lifecycle)
    pub fn retract_review(
        &self,
        review_id: Uuid,
        caller_wallet: &str,
        reason: Option<String>,
    ) -> Result<RetractionAudit, RetractionError> {
        let norm_caller = canonical_wallet(caller_wallet);

        let mut reviews = self.reviews.write().unwrap();
        let review = reviews
            .get_mut(&review_id)
            .ok_or_else(|| RetractionError::NotFound("review not found".to_string()))?;

        // Check if already retracted
        if review.is_deleted() {
            return Err(RetractionError::Gone(
                "review has already been retracted".to_string(),
            ));
        }

        // Authorization: only the original author or admin can retract
        let is_author = canonical_wallet(&review.author_wallet) == norm_caller;
        let is_admin_caller = self.is_admin(&norm_caller);

        if !is_author && !is_admin_caller {
            return Err(RetractionError::Forbidden(
                "only review author can retract this review".to_string(),
            ));
        }

        let now = Utc::now();
        review.deleted_at = Some(now);
        review.deleted_reason = reason.clone();
        review.updated_at = now;

        let audit = RetractionAudit {
            id: Uuid::new_v4(),
            review_id,
            retracted_by_wallet: norm_caller,
            retracted_at: now,
            reason,
        };

        let mut audits = self.audits.write().unwrap();
        audits.push(audit.clone());

        Ok(audit)
    }

    /// Fetch a single review by ID with 410 Gone semantics
    pub fn get_review(&self, review_id: Uuid) -> Result<RetractableReview, RetractionError> {
        let reviews = self.reviews.read().unwrap();
        let review = reviews
            .get(&review_id)
            .ok_or_else(|| RetractionError::NotFound("review not found".to_string()))?;

        if review.is_deleted() {
            return Err(RetractionError::Gone("Review retracted".to_string()));
        }

        Ok(review.clone())
    }

    /// List public company feed (strictly excludes retracted reviews)
    pub fn list_public_feed(
        &self,
        company_slug: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> CompanyFeedResponse {
        let slug = canonical_slug(company_slug);
        let reviews = self.reviews.read().unwrap();
        let mut active_reviews: Vec<RetractableReview> = reviews
            .values()
            .filter(|r| r.company_slug == slug && !r.is_deleted())
            .cloned()
            .collect();

        // Sort chronologically descending
        active_reviews.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let total_reviews = active_reviews.len();
        let l = limit.unwrap_or(10).min(50);
        let off = offset.unwrap_or(0);

        let paged_reviews = if off >= total_reviews {
            Vec::new()
        } else {
            let end = (off + l).min(total_reviews);
            active_reviews[off..end].to_vec()
        };

        CompanyFeedResponse {
            company_slug: slug,
            total_reviews,
            reviews: paged_reviews,
        }
    }

    /// Admin audit listing of retracted reviews with privacy-preserving anonymization
    pub fn admin_list_retracted(
        &self,
        caller_wallet: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<AdminRetractionAuditResponse, RetractionError> {
        if !self.is_admin(caller_wallet) {
            return Err(RetractionError::Forbidden(
                "admin privileges required".to_string(),
            ));
        }

        let audits = self.audits.read().unwrap();
        let total = audits.len();

        let l = limit.unwrap_or(20).min(100);
        let off = offset.unwrap_or(0);

        let paged: Vec<AdminRetractionAuditItem> = if off >= total {
            Vec::new()
        } else {
            let end = (off + l).min(total);
            audits[off..end]
                .iter()
                .map(|a| AdminRetractionAuditItem {
                    id: a.id,
                    review_id: a.review_id,
                    anonymized_author: anonymize_wallet(&a.retracted_by_wallet),
                    retracted_at: a.retracted_at,
                    reason: a.reason.clone(),
                })
                .collect()
        };

        Ok(AdminRetractionAuditResponse {
            total,
            audits: paged,
        })
    }
}
