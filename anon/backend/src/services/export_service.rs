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
    models::export::{
        ExportDataPayload, ExportJob, ExportStatus, ExportStatusResponse, ExportedReview,
    },
};

pub const TOKEN_EXPIRATION_HOURS: i64 = 1;
pub const RATE_LIMIT_HOURS: i64 = 24;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ExportError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Too many requests: {0}")]
    TooManyRequests(String),
    #[error("Expired: {0}")]
    Gone(String),
    #[error("Internal: {0}")]
    Internal(String),
}

impl IntoResponse for ExportError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            ExportError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            ExportError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            ExportError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            ExportError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            ExportError::TooManyRequests(m) => (StatusCode::TOO_MANY_REQUESTS, m.clone()),
            ExportError::Gone(m) => (StatusCode::GONE, m.clone()),
            ExportError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
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

/// Thread-safe in-memory store for data export requests and payloads
pub struct ExportService {
    jobs: RwLock<HashMap<Uuid, ExportJob>>,
    tokens: RwLock<HashMap<String, Uuid>>,
    user_reviews: RwLock<HashMap<String, Vec<ExportedReview>>>,
    last_export_times: RwLock<HashMap<String, DateTime<Utc>>>,
}

impl Default for ExportService {
    fn default() -> Self {
        Self::new()
    }
}

pub static EXPORT_SERVICE: LazyLock<ExportService> = LazyLock::new(ExportService::default);

impl ExportService {
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
            user_reviews: RwLock::new(HashMap::new()),
            last_export_times: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        self.jobs.write().unwrap().clear();
        self.tokens.write().unwrap().clear();
        self.user_reviews.write().unwrap().clear();
        self.last_export_times.write().unwrap().clear();
    }

    /// Seed a review for a user to export in tests
    pub fn add_user_review(&self, user_wallet: &str, review: ExportedReview) {
        let norm_wallet = canonical_wallet(user_wallet);
        let mut ur = self.user_reviews.write().unwrap();
        ur.entry(norm_wallet).or_default().push(review);
    }

    /// Request a new data export job
    pub fn request_export(&self, user_wallet: &str) -> Result<ExportStatusResponse, ExportError> {
        let norm_wallet = canonical_wallet(user_wallet);
        if norm_wallet.is_empty() {
            return Err(ExportError::BadRequest("invalid wallet address".to_string()));
        }

        let now = Utc::now();

        // Enforce 1 export per 24 hours rate limit
        {
            let last_times = self.last_export_times.read().unwrap();
            if let Some(last_time) = last_times.get(&norm_wallet) {
                let diff = now - *last_time;
                if diff < Duration::hours(RATE_LIMIT_HOURS) {
                    let wait_hours = RATE_LIMIT_HOURS - diff.num_hours();
                    return Err(ExportError::TooManyRequests(format!(
                        "rate limit exceeded: 1 export per 24h allowed. Please try again in ~{} hours",
                        wait_hours.max(1)
                    )));
                }
            }
        }

        // Gather reviews for this user
        let reviews = {
            let ur = self.user_reviews.read().unwrap();
            ur.get(&norm_wallet).cloned().unwrap_or_default()
        };

        let job_id = Uuid::new_v4();
        let token = Uuid::new_v4().to_string();
        let expires_at = now + Duration::hours(TOKEN_EXPIRATION_HOURS);
        let download_url = format!("/exports/download/{}", token);

        let job = ExportJob {
            id: job_id,
            user_wallet: norm_wallet.clone(),
            status: ExportStatus::Ready,
            download_url: Some(download_url.clone()),
            token: Some(token.clone()),
            expires_at: Some(expires_at),
            total_reviews: reviews.len(),
            created_at: now,
        };

        self.jobs.write().unwrap().insert(job_id, job);
        self.tokens.write().unwrap().insert(token, job_id);
        self.last_export_times
            .write()
            .unwrap()
            .insert(norm_wallet, now);

        Ok(ExportStatusResponse {
            job_id,
            status: ExportStatus::Ready,
            download_url: Some(download_url),
            expires_at: Some(expires_at),
            message: format!("Export ready. Contains {} reviews.", reviews.len()),
        })
    }

    /// Retrieve status of an export job
    pub fn get_export_status(
        &self,
        user_wallet: &str,
        job_id: Uuid,
    ) -> Result<ExportStatusResponse, ExportError> {
        let norm_wallet = canonical_wallet(user_wallet);
        let jobs = self.jobs.read().unwrap();

        let job = jobs
            .get(&job_id)
            .ok_or_else(|| ExportError::NotFound("export job not found".to_string()))?;

        if job.user_wallet != norm_wallet {
            return Err(ExportError::Forbidden(
                "you are not authorized to view this export job".to_string(),
            ));
        }

        // Check expiration
        let status = if let Some(exp) = job.expires_at {
            if Utc::now() > exp {
                ExportStatus::Error
            } else {
                job.status
            }
        } else {
            job.status
        };

        Ok(ExportStatusResponse {
            job_id: job.id,
            status,
            download_url: job.download_url.clone(),
            expires_at: job.expires_at,
            message: format!("Export status: {:?}", status),
        })
    }

    /// Download exported data by single-use/signed token
    pub fn download_export(&self, token: &str) -> Result<ExportDataPayload, ExportError> {
        let tokens = self.tokens.read().unwrap();
        let job_id = tokens
            .get(token)
            .ok_or_else(|| ExportError::NotFound("invalid or expired download token".to_string()))?;

        let jobs = self.jobs.read().unwrap();
        let job = jobs
            .get(job_id)
            .ok_or_else(|| ExportError::NotFound("export job not found".to_string()))?;

        if let Some(exp) = job.expires_at {
            if Utc::now() > exp {
                return Err(ExportError::Gone(
                    "download link has expired. Please request a new export.".to_string(),
                ));
            }
        }

        let ur = self.user_reviews.read().unwrap();
        let reviews = ur.get(&job.user_wallet).cloned().unwrap_or_default();

        Ok(ExportDataPayload {
            user_wallet: job.user_wallet.clone(),
            exported_at: Utc::now(),
            total: reviews.len(),
            reviews,
        })
    }
}
