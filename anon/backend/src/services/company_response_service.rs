use std::collections::{HashMap, HashSet};
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
    models::company_response::{CompanyResponse, CompanyResponseAudit},
};

pub const MAX_RESPONSE_LENGTH: usize = 5000;

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}").unwrap()
});

static PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+?\d{1,3}[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").unwrap()
});

static PRIVATE_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)0x[a-f0-9]{64}").unwrap()
});

static CREDIT_CARD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:\d{4}[ -]?){3}\d{4}\b").unwrap()
});

static SSN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap()
});

static SCRIPT_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\s*script[^>]*>[\s\S]*?<\s*/\s*script\s*>").unwrap()
});

static DANGEROUS_TAGS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<\s*/?\s*(?:iframe|object|embed|svg|style|link|meta|applet|form|input|button)[^>]*>").unwrap()
});

static EVENT_HANDLER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\s+on[a-z]+\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).unwrap()
});

static JAVASCRIPT_URI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)javascript:[^\s>]*").unwrap()
});

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ResponseError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for ResponseError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ResponseError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            ResponseError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            ResponseError::InvalidInput(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ResponseError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            ResponseError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

/// Canonicalize wallet address (normalizing hex format or lowercasing)
pub fn canonical_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim();
    crate::libs::wallet::normalize_and_validate(trimmed)
        .unwrap_or_else(|_| trimmed.to_lowercase())
}

/// Canonicalize company slug
pub fn canonical_slug(slug: &str) -> String {
    slug.trim().to_lowercase()
}

/// Sanitize response content from PII and potential XSS payloads
pub fn sanitize_response_text(raw: &str) -> String {
    let mut sanitized = raw.to_string();

    // 1. Strip script tags with content
    sanitized = SCRIPT_TAG_REGEX.replace_all(&sanitized, "").to_string();

    // 2. Strip dangerous tags
    sanitized = DANGEROUS_TAGS_REGEX.replace_all(&sanitized, "").to_string();

    // 3. Strip event handlers (e.g. onerror=, onload=)
    sanitized = EVENT_HANDLER_REGEX.replace_all(&sanitized, "").to_string();

    // 4. Strip javascript: URIs
    sanitized = JAVASCRIPT_URI_REGEX.replace_all(&sanitized, "").to_string();

    // 5. Redact PII - Private Keys
    sanitized = PRIVATE_KEY_REGEX.replace_all(&sanitized, "[REDACTED_KEY]").to_string();

    // 6. Redact PII - Credit Cards
    sanitized = CREDIT_CARD_REGEX.replace_all(&sanitized, "[REDACTED_CARD]").to_string();

    // 7. Redact PII - SSN
    sanitized = SSN_REGEX.replace_all(&sanitized, "[REDACTED_SSN]").to_string();

    // 8. Redact PII - Email Addresses
    sanitized = EMAIL_REGEX.replace_all(&sanitized, "[REDACTED_EMAIL]").to_string();

    // 9. Redact PII - Phone Numbers
    sanitized = PHONE_REGEX.replace_all(&sanitized, "[REDACTED_PHONE]").to_string();

    sanitized.trim().to_string()
}

/// In-memory thread-safe service managing company responses, ownership auth, and audit history
pub struct CompanyResponseService {
    company_owners: RwLock<HashMap<String, HashSet<String>>>,
    review_companies: RwLock<HashMap<Uuid, String>>,
    responses: RwLock<HashMap<Uuid, CompanyResponse>>,
    audit_history: RwLock<HashMap<Uuid, Vec<CompanyResponseAudit>>>,
}

impl Default for CompanyResponseService {
    fn default() -> Self {
        Self::new()
    }
}

pub static COMPANY_RESPONSE_SERVICE: LazyLock<CompanyResponseService> =
    LazyLock::new(CompanyResponseService::default);

impl CompanyResponseService {
    pub fn new() -> Self {
        Self {
            company_owners: RwLock::new(HashMap::new()),
            review_companies: RwLock::new(HashMap::new()),
            responses: RwLock::new(HashMap::new()),
            audit_history: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut co = self.company_owners.write().unwrap();
        co.clear();
        let mut rc = self.review_companies.write().unwrap();
        rc.clear();
        let mut r = self.responses.write().unwrap();
        r.clear();
        let mut a = self.audit_history.write().unwrap();
        a.clear();
    }

    /// Check if a wallet is registered as a verified owner for the company slug
    pub fn is_company_owner(&self, company_slug: &str, wallet: &str) -> bool {
        let slug = canonical_slug(company_slug);
        let norm_wallet = canonical_wallet(wallet);
        let owners = self.company_owners.read().unwrap();
        owners
            .get(&slug)
            .map(|set| set.contains(&norm_wallet))
            .unwrap_or(false)
    }

    /// Add a verified company owner
    pub fn add_company_owner(&self, company_slug: &str, wallet: &str) {
        let slug = canonical_slug(company_slug);
        let norm_wallet = canonical_wallet(wallet);
        let mut owners = self.company_owners.write().unwrap();
        owners.entry(slug).or_default().insert(norm_wallet);
    }

    /// Remove a verified company owner
    pub fn remove_company_owner(&self, company_slug: &str, wallet: &str) -> bool {
        let slug = canonical_slug(company_slug);
        let norm_wallet = canonical_wallet(wallet);
        let mut owners = self.company_owners.write().unwrap();
        owners
            .get_mut(&slug)
            .map(|set| set.remove(&norm_wallet))
            .unwrap_or(false)
    }

    /// Register a review to a company slug
    pub fn register_review(&self, review_id: Uuid, company_slug: &str) {
        let slug = canonical_slug(company_slug);
        let mut rc = self.review_companies.write().unwrap();
        rc.insert(review_id, slug);
    }

    /// Retrieve the company slug associated with a review
    pub fn get_review_company(&self, review_id: Uuid) -> Option<String> {
        let rc = self.review_companies.read().unwrap();
        if let Some(slug) = rc.get(&review_id) {
            return Some(slug.clone());
        }
        let responses = self.responses.read().unwrap();
        responses.get(&review_id).map(|r| r.company_slug.clone())
    }

    /// Publish or update an official company response (standard signature required by specification)
    pub fn post_or_update_response(
        &self,
        caller_wallet: &str,
        review_id: Uuid,
        text: &str,
    ) -> Result<CompanyResponse, ResponseError> {
        self.post_or_update_response_with_slug(caller_wallet, review_id, text, None)
    }

    /// Publish or update an official company response with optional company slug resolution
    pub fn post_or_update_response_with_slug(
        &self,
        caller_wallet: &str,
        review_id: Uuid,
        text: &str,
        company_slug: Option<&str>,
    ) -> Result<CompanyResponse, ResponseError> {
        // 1. Resolve company slug for this review
        let resolved_slug = match self.get_review_company(review_id) {
            Some(s) => s,
            None => {
                if let Some(slug) = company_slug {
                    let c_slug = canonical_slug(slug);
                    if c_slug.is_empty() {
                        return Err(ResponseError::InvalidInput("company slug cannot be empty".to_string()));
                    }
                    self.register_review(review_id, &c_slug);
                    c_slug
                } else {
                    return Err(ResponseError::NotFound(
                        "review not found or company not associated".to_string(),
                    ));
                }
            }
        };

        // 2. Verify company owner authorization
        if !self.is_company_owner(&resolved_slug, caller_wallet) {
            return Err(ResponseError::Forbidden(
                "only verified company owners can publish official responses".to_string(),
            ));
        }

        // 3. Text length and emptiness validation
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(ResponseError::InvalidInput(
                "response text cannot be empty".to_string(),
            ));
        }
        if trimmed.chars().count() > MAX_RESPONSE_LENGTH {
            return Err(ResponseError::InvalidInput(format!(
                "response text exceeds maximum length of {} characters",
                MAX_RESPONSE_LENGTH
            )));
        }

        // 4. Sanitize PII and XSS
        let sanitized = sanitize_response_text(trimmed);
        if sanitized.is_empty() {
            return Err(ResponseError::InvalidInput(
                "response text cannot be empty after sanitization".to_string(),
            ));
        }

        let norm_wallet = canonical_wallet(caller_wallet);
        let now = Utc::now();

        let mut responses = self.responses.write().unwrap();
        let mut audits = self.audit_history.write().unwrap();

        // 5. 1:1 Response per review: update if existing, create if new
        if let Some(existing) = responses.get_mut(&review_id) {
            // Record audit entry
            let audit = CompanyResponseAudit {
                id: Uuid::new_v4(),
                response_id: existing.id,
                previous_text: existing.text.clone(),
                new_text: sanitized.clone(),
                editor_wallet: norm_wallet.clone(),
                edited_at: now,
            };
            audits.entry(existing.id).or_default().push(audit);

            // Update existing response
            existing.text = sanitized;
            existing.owner_wallet = norm_wallet;
            existing.updated_at = now;

            Ok(existing.clone())
        } else {
            // Create new response
            let response = CompanyResponse {
                id: Uuid::new_v4(),
                review_id,
                company_slug: resolved_slug,
                owner_wallet: norm_wallet,
                text: sanitized,
                created_at: now,
                updated_at: now,
            };
            responses.insert(review_id, response.clone());
            Ok(response)
        }
    }

    /// Retrieve the official company response for a review
    pub fn get_response_for_review(&self, review_id: Uuid) -> Option<CompanyResponse> {
        let responses = self.responses.read().unwrap();
        responses.get(&review_id).cloned()
    }

    /// Retrieve a company response by its response ID
    pub fn get_response_by_id(&self, response_id: Uuid) -> Option<CompanyResponse> {
        let responses = self.responses.read().unwrap();
        responses.values().find(|r| r.id == response_id).cloned()
    }

    /// Retrieve audit history for a given response ID
    pub fn get_response_audit_history(&self, response_id: Uuid) -> Vec<CompanyResponseAudit> {
        let audits = self.audit_history.read().unwrap();
        audits.get(&response_id).cloned().unwrap_or_default()
    }

    /// Retrieve audit history for a review's response
    pub fn get_response_audit_for_review(
        &self,
        review_id: Uuid,
    ) -> Result<Vec<CompanyResponseAudit>, ResponseError> {
        let resp = self
            .get_response_for_review(review_id)
            .ok_or_else(|| ResponseError::NotFound("official response not found for review".to_string()))?;
        Ok(self.get_response_audit_history(resp.id))
    }
}
