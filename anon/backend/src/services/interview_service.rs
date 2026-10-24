use std::collections::HashMap;
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
    models::interview::{
        CreateInterviewRequest, InterviewListResponse, InterviewQuestion, InterviewReview,
    },
};

pub const DEFAULT_LIMIT: usize = 10;
pub const MAX_LIMIT: usize = 50;
pub const MIN_PROCESS_SUMMARY_LEN: usize = 10;
pub const MAX_PROCESS_SUMMARY_LEN: usize = 5000;
pub const MAX_ROLE_TITLE_LEN: usize = 150;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum InterviewError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for InterviewError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            InterviewError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            InterviewError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            InterviewError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            InterviewError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
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

/// Thread-safe in-memory store for interview reviews
pub struct InterviewService {
    interviews: RwLock<HashMap<Uuid, InterviewReview>>,
}

impl Default for InterviewService {
    fn default() -> Self {
        Self::new()
    }
}

pub static INTERVIEW_SERVICE: LazyLock<InterviewService> =
    LazyLock::new(InterviewService::default);

impl InterviewService {
    pub fn new() -> Self {
        Self {
            interviews: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut iv = self.interviews.write().unwrap();
        iv.clear();
    }

    /// Create and persist a new interview review
    pub fn create_interview(
        &self,
        author_wallet: &str,
        req: CreateInterviewRequest,
    ) -> Result<InterviewReview, InterviewError> {
        let norm_wallet = canonical_wallet(author_wallet);
        if norm_wallet.is_empty() {
            return Err(InterviewError::BadRequest("invalid author wallet".to_string()));
        }

        let slug = canonical_slug(&req.company_slug);
        if slug.is_empty() {
            return Err(InterviewError::BadRequest("company_slug cannot be empty".to_string()));
        }

        let role_title = req.role_title.trim().to_string();
        if role_title.len() < 2 || role_title.chars().count() > MAX_ROLE_TITLE_LEN {
            return Err(InterviewError::BadRequest(format!(
                "role_title must be between 2 and {} characters",
                MAX_ROLE_TITLE_LEN
            )));
        }

        if req.difficulty < 1 || req.difficulty > 5 {
            return Err(InterviewError::BadRequest(
                "difficulty must be an integer between 1 and 5".to_string(),
            ));
        }

        let trimmed_summary = req.process_summary.trim();
        if trimmed_summary.len() < MIN_PROCESS_SUMMARY_LEN
            || trimmed_summary.chars().count() > MAX_PROCESS_SUMMARY_LEN
        {
            return Err(InterviewError::BadRequest(format!(
                "process_summary must be between {} and {} characters",
                MIN_PROCESS_SUMMARY_LEN, MAX_PROCESS_SUMMARY_LEN
            )));
        }

        let sanitized_summary = sanitize_text(trimmed_summary);

        let sanitized_questions: Vec<InterviewQuestion> = req
            .questions
            .into_iter()
            .map(|q| InterviewQuestion {
                question: sanitize_text(q.question.trim()),
                category: q.category.map(|c| c.trim().to_lowercase()),
            })
            .collect();

        let review_id = Uuid::new_v4();
        let review = InterviewReview {
            id: review_id,
            company_slug: slug,
            author_wallet: norm_wallet,
            role_title,
            location: req.location.map(|l| l.trim().to_string()),
            experience_level: req.experience_level.map(|e| e.trim().to_string()),
            overall_experience: req.overall_experience,
            difficulty: req.difficulty,
            process_summary: sanitized_summary,
            questions: sanitized_questions,
            timeframe: req.timeframe.map(|t| t.trim().to_string()),
            created_at: Utc::now(),
        };

        let mut iv = self.interviews.write().unwrap();
        iv.insert(review_id, review.clone());

        Ok(review)
    }

    /// Retrieve an interview review by unique ID
    pub fn get_interview(&self, id: Uuid) -> Result<InterviewReview, InterviewError> {
        let iv = self.interviews.read().unwrap();
        iv.get(&id)
            .cloned()
            .ok_or_else(|| InterviewError::NotFound("interview review not found".to_string()))
    }

    /// List interview reviews for a company with cursor pagination
    pub fn list_company_interviews(
        &self,
        company_slug: &str,
        limit_opt: Option<usize>,
        cursor_opt: Option<String>,
    ) -> Result<InterviewListResponse, InterviewError> {
        let slug = canonical_slug(company_slug);
        let limit = limit_opt.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

        let iv = self.interviews.read().unwrap();
        let mut list: Vec<InterviewReview> = iv
            .values()
            .filter(|r| r.company_slug == slug)
            .cloned()
            .collect();

        // Sort descending by created_at, secondary by ID
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));

        let total = list.len();

        let start_idx = if let Some(cursor_str) = cursor_opt {
            let cursor_uuid = Uuid::parse_str(&cursor_str)
                .map_err(|_| InterviewError::BadRequest("invalid cursor format".to_string()))?;
            match list.iter().position(|r| r.id == cursor_uuid) {
                Some(pos) => pos + 1,
                None => 0,
            }
        } else {
            0
        };

        if start_idx >= total {
            return Ok(InterviewListResponse {
                items: Vec::new(),
                total,
                next_cursor: None,
                has_more: false,
            });
        }

        let end_idx = (start_idx + limit).min(total);
        let page_items = list[start_idx..end_idx].to_vec();
        let has_more = end_idx < total;
        let next_cursor = if has_more && !page_items.is_empty() {
            Some(page_items.last().unwrap().id.to_string())
        } else {
            None
        };

        Ok(InterviewListResponse {
            items: page_items,
            total,
            next_cursor,
            has_more,
        })
    }
}
