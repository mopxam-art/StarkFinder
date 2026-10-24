use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::{
    libs::error::ErrorBody,
    models::draft::{
        CreateDraftDto, PublishedReview, ReviewAuditEntry, ReviewDraft, UpdateDraftDto,
    },
};

pub const DEFAULT_EDIT_WINDOW_SECONDS: i64 = 900; // 15 minutes
pub const MAX_TITLE_LENGTH: usize = 200;
pub const MAX_CONTENT_LENGTH: usize = 10000;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum DraftError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Edit window expired: {0}")]
    EditWindowExpired(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for DraftError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            DraftError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            DraftError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            DraftError::EditWindowExpired(m) => (StatusCode::BAD_REQUEST, m.clone()),
            DraftError::InvalidInput(m) => (StatusCode::BAD_REQUEST, m.clone()),
            DraftError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

/// In-memory thread-safe store for drafts, published reviews, and audit trail
pub struct DraftService {
    drafts: RwLock<HashMap<Uuid, ReviewDraft>>,
    published_reviews: RwLock<HashMap<Uuid, PublishedReview>>,
    audit_history: RwLock<HashMap<Uuid, Vec<ReviewAuditEntry>>>,
    edit_window_seconds: RwLock<i64>,
}

impl Default for DraftService {
    fn default() -> Self {
        Self::new()
    }
}

pub static DRAFT_SERVICE: LazyLock<DraftService> = LazyLock::new(DraftService::default);

impl DraftService {
    pub fn new() -> Self {
        Self {
            drafts: RwLock::new(HashMap::new()),
            published_reviews: RwLock::new(HashMap::new()),
            audit_history: RwLock::new(HashMap::new()),
            edit_window_seconds: RwLock::new(DEFAULT_EDIT_WINDOW_SECONDS),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut d = self.drafts.write().unwrap();
        d.clear();
        let mut p = self.published_reviews.write().unwrap();
        p.clear();
        let mut a = self.audit_history.write().unwrap();
        a.clear();
        let mut w = self.edit_window_seconds.write().unwrap();
        *w = DEFAULT_EDIT_WINDOW_SECONDS;
    }

    /// Configure custom edit window duration in seconds (for fast expiration testing)
    pub fn set_edit_window_seconds(&self, seconds: i64) {
        let mut w = self.edit_window_seconds.write().unwrap();
        *w = seconds;
    }

    /// Override published_at timestamp for a review (for expiration testing)
    pub fn set_review_published_at(&self, review_id: Uuid, published_at: DateTime<Utc>) -> Result<(), DraftError> {
        let mut p = self.published_reviews.write().unwrap();
        let rev = p
            .get_mut(&review_id)
            .ok_or_else(|| DraftError::NotFound("Review not found".to_string()))?;
        rev.published_at = published_at;
        Ok(())
    }

    /// Create a new review draft
    pub fn create_draft(
        &self,
        author_wallet: &str,
        req: CreateDraftDto,
    ) -> Result<ReviewDraft, DraftError> {
        let norm_wallet = canonical_wallet(author_wallet);
        let slug = req.company_slug.trim().to_lowercase();
        if slug.is_empty() {
            return Err(DraftError::InvalidInput("Company slug cannot be empty".into()));
        }

        let title = req.title.trim().to_string();
        if title.is_empty() {
            return Err(DraftError::InvalidInput("Review title cannot be empty".into()));
        }
        if title.len() > MAX_TITLE_LENGTH {
            return Err(DraftError::InvalidInput(format!(
                "Title exceeds max length of {} characters",
                MAX_TITLE_LENGTH
            )));
        }

        let content = req.content.trim().to_string();
        if content.is_empty() {
            return Err(DraftError::InvalidInput("Review content cannot be empty".into()));
        }
        if content.len() > MAX_CONTENT_LENGTH {
            return Err(DraftError::InvalidInput(format!(
                "Content exceeds max length of {} characters",
                MAX_CONTENT_LENGTH
            )));
        }

        if !(1..=5).contains(&req.rating) {
            return Err(DraftError::InvalidInput(
                "Rating must be between 1 and 5 stars".into(),
            ));
        }

        let now = Utc::now();
        let draft = ReviewDraft {
            id: Uuid::new_v4(),
            author_wallet: norm_wallet,
            company_slug: slug,
            title,
            content,
            rating: req.rating,
            tags: req.tags.into_iter().map(|t| t.trim().to_lowercase()).collect(),
            created_at: now,
            updated_at: now,
        };

        {
            let mut d = self.drafts.write().unwrap();
            d.insert(draft.id, draft.clone());
        }

        Ok(draft)
    }

    /// Update an existing draft by author
    pub fn update_draft(
        &self,
        author_wallet: &str,
        draft_id: Uuid,
        req: UpdateDraftDto,
    ) -> Result<ReviewDraft, DraftError> {
        let norm_wallet = canonical_wallet(author_wallet);
        let mut d = self.drafts.write().unwrap();

        let draft = d
            .get_mut(&draft_id)
            .ok_or_else(|| DraftError::NotFound("Draft not found".to_string()))?;

        // Author ownership check
        if draft.author_wallet != norm_wallet {
            return Err(DraftError::Forbidden(
                "Access denied: only author can update this draft".to_string(),
            ));
        }

        if let Some(slug) = req.company_slug {
            let s = slug.trim().to_lowercase();
            if s.is_empty() {
                return Err(DraftError::InvalidInput("Company slug cannot be empty".into()));
            }
            draft.company_slug = s;
        }

        if let Some(t) = req.title {
            let trimmed = t.trim().to_string();
            if trimmed.is_empty() {
                return Err(DraftError::InvalidInput("Title cannot be empty".into()));
            }
            draft.title = trimmed;
        }

        if let Some(c) = req.content {
            let trimmed = c.trim().to_string();
            if trimmed.is_empty() {
                return Err(DraftError::InvalidInput("Content cannot be empty".into()));
            }
            draft.content = trimmed;
        }

        if let Some(r) = req.rating {
            if !(1..=5).contains(&r) {
                return Err(DraftError::InvalidInput("Rating must be between 1 and 5".into()));
            }
            draft.rating = r;
        }

        if let Some(tags) = req.tags {
            draft.tags = tags.into_iter().map(|t| t.trim().to_lowercase()).collect();
        }

        draft.updated_at = Utc::now();
        Ok(draft.clone())
    }

    /// Delete a draft by author
    pub fn delete_draft(&self, author_wallet: &str, draft_id: Uuid) -> Result<(), DraftError> {
        let norm_wallet = canonical_wallet(author_wallet);
        let mut d = self.drafts.write().unwrap();

        let draft = d
            .get(&draft_id)
            .ok_or_else(|| DraftError::NotFound("Draft not found".to_string()))?;

        if draft.author_wallet != norm_wallet {
            return Err(DraftError::Forbidden(
                "Access denied: only author can delete this draft".to_string(),
            ));
        }

        d.remove(&draft_id);
        Ok(())
    }

    /// Publish a draft into a live review (removes draft from draft storage)
    pub fn publish_draft(
        &self,
        author_wallet: &str,
        draft_id: Uuid,
    ) -> Result<PublishedReview, DraftError> {
        let norm_wallet = canonical_wallet(author_wallet);
        let draft = {
            let mut d = self.drafts.write().unwrap();
            let draft = d
                .get(&draft_id)
                .ok_or_else(|| DraftError::NotFound("Draft not found".to_string()))?;

            if draft.author_wallet != norm_wallet {
                return Err(DraftError::Forbidden(
                    "Access denied: only author can publish this draft".to_string(),
                ));
            }

            d.remove(&draft_id).unwrap()
        };

        let now = Utc::now();
        let published = PublishedReview {
            id: draft.id,
            author_wallet: draft.author_wallet,
            company_slug: draft.company_slug,
            title: draft.title,
            content: draft.content,
            rating: draft.rating,
            tags: draft.tags,
            published_at: now,
            updated_at: now,
            is_edited: false,
        };

        {
            let mut p = self.published_reviews.write().unwrap();
            p.insert(published.id, published.clone());
        }

        Ok(published)
    }

    /// Edit a published review within the allowed time window (records audit log)
    pub fn edit_published_review(
        &self,
        author_wallet: &str,
        review_id: Uuid,
        new_content: &str,
        edit_reason: Option<String>,
    ) -> Result<(PublishedReview, ReviewAuditEntry), DraftError> {
        let norm_wallet = canonical_wallet(author_wallet);
        let trimmed_content = new_content.trim();
        if trimmed_content.is_empty() {
            return Err(DraftError::InvalidInput("Content cannot be empty".into()));
        }

        let mut p = self.published_reviews.write().unwrap();
        let review = p
            .get_mut(&review_id)
            .ok_or_else(|| DraftError::NotFound("Review not found".to_string()))?;

        // 1. Author check
        if review.author_wallet != norm_wallet {
            return Err(DraftError::Forbidden(
                "Access denied: only the review author can edit this review".to_string(),
            ));
        }

        // 2. Edit window check
        let now = Utc::now();
        let window_secs = *self.edit_window_seconds.read().unwrap();
        let elapsed = now.signed_duration_since(review.published_at);
        if elapsed > Duration::seconds(window_secs) {
            return Err(DraftError::EditWindowExpired(format!(
                "Edit window expired: review was published {} seconds ago (limit: {} seconds)",
                elapsed.num_seconds(),
                window_secs
            )));
        }

        // 3. Create immutable audit entry
        let audit_entry = ReviewAuditEntry {
            id: Uuid::new_v4(),
            review_id,
            editor_wallet: norm_wallet,
            previous_content: review.content.clone(),
            new_content: trimmed_content.to_string(),
            edit_reason,
            edited_at: now,
        };

        {
            let mut a = self.audit_history.write().unwrap();
            a.entry(review_id).or_default().push(audit_entry.clone());
        }

        // 4. Update review content
        review.content = trimmed_content.to_string();
        review.updated_at = now;
        review.is_edited = true;

        Ok((review.clone(), audit_entry))
    }

    /// Retrieve single draft by ID
    pub fn get_draft(&self, author_wallet: &str, draft_id: Uuid) -> Result<ReviewDraft, DraftError> {
        let norm_wallet = canonical_wallet(author_wallet);
        let d = self.drafts.read().unwrap();
        let draft = d
            .get(&draft_id)
            .ok_or_else(|| DraftError::NotFound("Draft not found".to_string()))?;

        if draft.author_wallet != norm_wallet {
            return Err(DraftError::Forbidden(
                "Access denied: cannot access another user's draft".to_string(),
            ));
        }

        Ok(draft.clone())
    }

    /// List all drafts for an author
    pub fn list_user_drafts(&self, author_wallet: &str) -> Vec<ReviewDraft> {
        let norm_wallet = canonical_wallet(author_wallet);
        let d = self.drafts.read().unwrap();
        let mut list: Vec<ReviewDraft> = d
            .values()
            .filter(|draft| draft.author_wallet == norm_wallet)
            .cloned()
            .collect();

        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    /// Retrieve published review by ID
    pub fn get_published_review(&self, review_id: Uuid) -> Result<PublishedReview, DraftError> {
        let p = self.published_reviews.read().unwrap();
        p.get(&review_id)
            .cloned()
            .ok_or_else(|| DraftError::NotFound("Published review not found".to_string()))
    }

    /// Retrieve audit history for a review
    pub fn get_review_audit_history(&self, review_id: Uuid) -> Vec<ReviewAuditEntry> {
        let a = self.audit_history.read().unwrap();
        a.get(&review_id).cloned().unwrap_or_default()
    }
}

/// Helper: canonicalize wallet address
pub fn canonical_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    crate::libs::wallet::normalize_and_validate(trimmed)
        .unwrap_or_else(|_| trimmed.to_lowercase())
}
