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
    models::appeal::{
        Appeal, AppealListResponse, AppealStatus, CreateAppealRequest,
        ResolveAppealRequest,
    },
};

pub const MIN_REASON_LEN: usize = 10;
pub const MAX_REASON_LEN: usize = 2000;
pub const MIN_NOTE_LEN: usize = 5;
pub const MAX_NOTE_LEN: usize = 2000;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum AppealError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppealError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppealError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppealError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            AppealError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            AppealError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            AppealError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
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

/// Thread-safe in-memory store for appeals
pub struct AppealService {
    appeals: RwLock<HashMap<Uuid, Appeal>>,
}

impl Default for AppealService {
    fn default() -> Self {
        Self::new()
    }
}

pub static APPEAL_SERVICE: LazyLock<AppealService> = LazyLock::new(AppealService::default);

impl AppealService {
    pub fn new() -> Self {
        Self {
            appeals: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut app = self.appeals.write().unwrap();
        app.clear();
    }

    /// Submit a new moderation appeal
    pub fn create_appeal(
        &self,
        wallet: &str,
        req: CreateAppealRequest,
    ) -> Result<Appeal, AppealError> {
        let norm_wallet = canonical_wallet(wallet);
        if norm_wallet.is_empty() {
            return Err(AppealError::BadRequest("invalid appellant wallet".to_string()));
        }

        let trimmed_reason = req.reason.trim();
        if trimmed_reason.len() < MIN_REASON_LEN
            || trimmed_reason.chars().count() > MAX_REASON_LEN
        {
            return Err(AppealError::BadRequest(format!(
                "reason must be between {} and {} characters",
                MIN_REASON_LEN, MAX_REASON_LEN
            )));
        }

        let mut app = self.appeals.write().unwrap();

        // Check if an open appeal already exists for this review and actor
        let has_open = app
            .values()
            .any(|a| a.review_id == req.review_id && a.actor == req.actor && a.status == AppealStatus::Open);

        if has_open {
            return Err(AppealError::Conflict(
                "an open appeal already exists for this review and actor".to_string(),
            ));
        }

        let sanitized_reason = sanitize_text(trimmed_reason);
        let appeal_id = Uuid::new_v4();
        let appeal = Appeal {
            id: appeal_id,
            review_id: req.review_id,
            actor: req.actor,
            appellant_wallet: norm_wallet,
            reason: sanitized_reason,
            status: AppealStatus::Open,
            resolution_note: None,
            created_at: Utc::now(),
            resolved_at: None,
        };

        app.insert(appeal_id, appeal.clone());
        Ok(appeal)
    }

    /// Retrieve an appeal by ID
    pub fn get_appeal(&self, id: Uuid) -> Result<Appeal, AppealError> {
        let app = self.appeals.read().unwrap();
        app.get(&id)
            .cloned()
            .ok_or_else(|| AppealError::NotFound("appeal not found".to_string()))
    }

    /// List appeals with optional status filter, ordered by created_at DESC
    pub fn list_appeals(
        &self,
        status_filter: Option<AppealStatus>,
    ) -> Result<AppealListResponse, AppealError> {
        let app = self.appeals.read().unwrap();
        let mut list: Vec<Appeal> = app
            .values()
            .filter(|a| {
                if let Some(status) = status_filter {
                    a.status == status
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        // Sort descending by created_at, secondary by ID
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));

        let total = list.len();
        Ok(AppealListResponse { items: list, total })
    }

    /// Admin resolution of an appeal
    pub fn resolve_appeal(
        &self,
        id: Uuid,
        req: ResolveAppealRequest,
    ) -> Result<Appeal, AppealError> {
        if req.status == AppealStatus::Open {
            return Err(AppealError::BadRequest(
                "resolution status must be accepted or rejected".to_string(),
            ));
        }

        let trimmed_note = req.resolution_note.trim();
        if trimmed_note.len() < MIN_NOTE_LEN || trimmed_note.chars().count() > MAX_NOTE_LEN {
            return Err(AppealError::BadRequest(format!(
                "resolution_note must be between {} and {} characters",
                MIN_NOTE_LEN, MAX_NOTE_LEN
            )));
        }

        let mut app = self.appeals.write().unwrap();
        let appeal = app
            .get_mut(&id)
            .ok_or_else(|| AppealError::NotFound("appeal not found".to_string()))?;

        if appeal.status != AppealStatus::Open {
            return Err(AppealError::BadRequest(
                "cannot resolve an appeal that is already resolved".to_string(),
            ));
        }

        let sanitized_note = sanitize_text(trimmed_note);
        appeal.status = req.status;
        appeal.resolution_note = Some(sanitized_note);
        appeal.resolved_at = Some(Utc::now());

        Ok(appeal.clone())
    }
}
