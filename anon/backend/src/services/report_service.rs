use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, RwLock};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use regex::Regex;
use uuid::Uuid;

use crate::{
    libs::error::ErrorBody,
    models::report::{
        AdminReportsResponse, CreateReportRequest, CreateReportResponse, ReportStatus,
        ResolveReportRequest, ReviewReport,
    },
};

pub const MAX_NOTE_LENGTH: usize = 1000;

static SCRIPT_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\s*script[^>]*>[\s\S]*?<\s*/\s*script\s*>").unwrap()
});

static DANGEROUS_TAGS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\s*/?\s*(?:iframe|object|embed|svg|style|link|meta|applet|form|input|button)[^>]*>").unwrap()
});

static EVENT_HANDLER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\s+on[a-z]+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).unwrap()
});

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ReportError {
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

impl IntoResponse for ReportError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ReportError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            ReportError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            ReportError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            ReportError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ReportError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            ReportError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

/// Canonicalize wallet address
pub fn canonical_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim();
    crate::libs::wallet::normalize_and_validate(trimmed)
        .unwrap_or_else(|_| trimmed.to_lowercase())
}

/// Pseudonymize reporter identity to protect against de-anonymization
pub fn hash_reporter_id(wallet: &str) -> String {
    let norm = canonical_wallet(wallet);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    norm.hash(&mut hasher);
    let h = hasher.finish();
    format!("rep_{:016x}", h)
}

/// Sanitize optional user note from XSS payloads
pub fn sanitize_note(raw: &str) -> String {
    let mut s = raw.to_string();
    s = SCRIPT_TAG_REGEX.replace_all(&s, "").to_string();
    s = DANGEROUS_TAGS_REGEX.replace_all(&s, "").to_string();
    s = EVENT_HANDLER_REGEX.replace_all(&s, "").to_string();
    s.trim().to_string()
}

/// Thread-safe in-memory store for moderation reports and admin reviews queue
pub struct ReportService {
    reports: RwLock<HashMap<Uuid, ReviewReport>>,
    registered_reviews: RwLock<HashSet<Uuid>>,
    removed_reviews: RwLock<HashSet<Uuid>>,
    admin_wallets: RwLock<HashSet<String>>,
}

impl Default for ReportService {
    fn default() -> Self {
        Self::new()
    }
}

pub static REPORT_SERVICE: LazyLock<ReportService> = LazyLock::new(ReportService::default);

impl ReportService {
    pub fn new() -> Self {
        Self {
            reports: RwLock::new(HashMap::new()),
            registered_reviews: RwLock::new(HashSet::new()),
            removed_reviews: RwLock::new(HashSet::new()),
            admin_wallets: RwLock::new(HashSet::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut rep = self.reports.write().unwrap();
        rep.clear();
        let mut rr = self.registered_reviews.write().unwrap();
        rr.clear();
        let mut rem = self.removed_reviews.write().unwrap();
        rem.clear();
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

    /// Register a valid review ID
    pub fn register_review(&self, review_id: Uuid) {
        let mut rr = self.registered_reviews.write().unwrap();
        rr.insert(review_id);
    }

    /// Check if a review has been registered
    pub fn is_review_registered(&self, review_id: Uuid) -> bool {
        let rr = self.registered_reviews.read().unwrap();
        rr.contains(&review_id)
    }

    /// Check if a review was removed as a result of moderation resolution
    pub fn is_review_removed(&self, review_id: Uuid) -> bool {
        let rem = self.removed_reviews.read().unwrap();
        rem.contains(&review_id)
    }

    /// Create a new moderation report
    pub fn create_report(
        &self,
        req: CreateReportRequest,
        reporter_wallet: &str,
    ) -> Result<CreateReportResponse, ReportError> {
        // 1. Verify existence of review
        if !self.is_review_registered(req.review_id) {
            return Err(ReportError::NotFound("review not found".to_string()));
        }

        // 2. Validate note length
        let sanitized_note = if let Some(ref note) = req.note {
            if note.chars().count() > MAX_NOTE_LENGTH {
                return Err(ReportError::BadRequest(format!(
                    "note exceeds maximum length of {} characters",
                    MAX_NOTE_LENGTH
                )));
            }
            Some(sanitize_note(note))
        } else {
            None
        };

        let hashed_reporter = hash_reporter_id(reporter_wallet);
        let mut reports = self.reports.write().unwrap();

        // 3. Duplicate check: same reporter on same review with active status (Open or InReview)
        let is_duplicate = reports.values().any(|r| {
            r.review_id == req.review_id
                && r.reporter_user_id == hashed_reporter
                && (r.status == ReportStatus::Open || r.status == ReportStatus::InReview)
        });

        if is_duplicate {
            return Err(ReportError::Conflict(
                "duplicate active report on this review".to_string(),
            ));
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        let report = ReviewReport {
            id,
            review_id: req.review_id,
            reporter_user_id: hashed_reporter,
            reason_code: req.reason_code,
            note: sanitized_note,
            status: ReportStatus::Open,
            resolution_note: None,
            resolved_by: None,
            created_at: now,
            updated_at: now,
        };

        reports.insert(id, report);

        Ok(CreateReportResponse {
            id,
            status: ReportStatus::Open,
            created_at: now,
        })
    }

    /// List reports for the admin queue with status filtering and pagination
    pub fn list_reports(
        &self,
        status_filter: Option<ReportStatus>,
        limit: Option<usize>,
        cursor: Option<&str>,
    ) -> AdminReportsResponse {
        let reports = self.reports.read().unwrap();
        let mut filtered: Vec<ReviewReport> = reports
            .values()
            .filter(|r| {
                if let Some(s) = status_filter {
                    r.status == s
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        // Sort by created_at descending, then id ascending
        filtered.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let total = filtered.len();
        let l = limit.unwrap_or(20).min(100);

        // Find cursor offset if cursor provided (cursor is report ID)
        let offset = if let Some(cur) = cursor {
            filtered
                .iter()
                .position(|r| r.id.to_string() == cur)
                .map(|pos| pos + 1)
                .unwrap_or(0)
        } else {
            0
        };

        let items = if offset >= total {
            Vec::new()
        } else {
            let end = (offset + l).min(total);
            filtered[offset..end].to_vec()
        };

        let next_cursor = items.last().map(|r| r.id.to_string());

        AdminReportsResponse {
            total,
            items,
            cursor: next_cursor,
        }
    }

    /// Resolve a report as an administrator
    pub fn resolve_report(
        &self,
        report_id: Uuid,
        admin_wallet: &str,
        req: ResolveReportRequest,
    ) -> Result<ReviewReport, ReportError> {
        let norm_admin = canonical_wallet(admin_wallet);

        let mut reports = self.reports.write().unwrap();
        let report = reports
            .get_mut(&report_id)
            .ok_or_else(|| ReportError::NotFound("report not found".to_string()))?;

        let now = Utc::now();
        report.status = req.status;
        report.resolution_note = req.resolution_note;
        report.resolved_by = Some(norm_admin);
        report.updated_at = now;

        // If status is ResolvedRemoved, mark review as removed
        if req.status == ReportStatus::ResolvedRemoved {
            let mut rem = self.removed_reviews.write().unwrap();
            rem.insert(report.review_id);
        }

        Ok(report.clone())
    }

    /// Retrieve a report by ID
    pub fn get_report(&self, report_id: Uuid) -> Option<ReviewReport> {
        let reports = self.reports.read().unwrap();
        reports.get(&report_id).cloned()
    }
}
