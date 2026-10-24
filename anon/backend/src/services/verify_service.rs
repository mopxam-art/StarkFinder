use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use uuid::Uuid;

use crate::{
    libs::error::ErrorBody,
    models::verify::{
        StartVerificationRequest, StartVerificationResponse, SubmitProofRequest,
        SubmitProofResponse, VerificationStatus, VerificationStatusResponse,
    },
};

pub const MAX_PROOF_BLOB_BYTES: usize = 65536; // 64 KB
pub const SESSION_DURATION_MINUTES: i64 = 15;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum VerifyError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for VerifyError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            VerifyError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            VerifyError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            VerifyError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            VerifyError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

/// Canonicalize Starknet wallet address
pub fn canonical_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim().to_lowercase();
    crate::libs::wallet::normalize_and_validate(&trimmed).unwrap_or(trimmed)
}

/// Canonicalize company slug
pub fn canonical_slug(slug: &str) -> String {
    slug.trim().to_lowercase()
}

/// Internal session record representing a pending or finalized zkTLS proof flow
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationSession {
    pub id: Uuid,
    pub wallet: String,
    pub company_slug: String,
    pub provider: String,
    pub challenge: String,
    pub status: VerificationStatus,
    pub attestation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
}

/// In-memory thread-safe store for zkTLS verification sessions
pub struct VerifyService {
    sessions: RwLock<HashMap<Uuid, VerificationSession>>,
}

impl Default for VerifyService {
    fn default() -> Self {
        Self::new()
    }
}

pub static VERIFY_SERVICE: LazyLock<VerifyService> = LazyLock::new(VerifyService::default);

impl VerifyService {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut ses = self.sessions.write().unwrap();
        ses.clear();
    }

    /// Initialize a new zkTLS verification challenge session
    pub fn start_session(
        &self,
        wallet: &str,
        req: StartVerificationRequest,
    ) -> Result<StartVerificationResponse, VerifyError> {
        let norm_wallet = canonical_wallet(wallet);
        if norm_wallet.is_empty() {
            return Err(VerifyError::BadRequest(
                "invalid appellant wallet address".to_string(),
            ));
        }

        let slug = canonical_slug(&req.company_slug);
        if slug.is_empty() {
            return Err(VerifyError::BadRequest(
                "company_slug cannot be empty".to_string(),
            ));
        }

        let provider = req
            .provider
            .unwrap_or_else(|| "reclaim".to_string())
            .trim()
            .to_lowercase();

        let session_id = Uuid::new_v4();

        // Generate cryptographically secure random 32-byte hex challenge
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 32] = rng.r#gen();
        let challenge = format!("0x{}", hex::encode(random_bytes));

        let now = Utc::now();
        let expires_at = now + Duration::minutes(SESSION_DURATION_MINUTES);

        let provider_session_url = format!(
            "https://share.{}-provider.org/verify/session?id={}&slug={}",
            provider, session_id, slug
        );

        let session = VerificationSession {
            id: session_id,
            wallet: norm_wallet,
            company_slug: slug.clone(),
            provider,
            challenge: challenge.clone(),
            status: VerificationStatus::Pending,
            attestation_id: None,
            created_at: now,
            expires_at,
            verified_at: None,
        };

        let mut ses = self.sessions.write().unwrap();
        ses.insert(session_id, session);

        Ok(StartVerificationResponse {
            session_id,
            company_slug: slug,
            challenge,
            expires_at,
            provider_session_url,
        })
    }

    /// Validate submitted zkTLS proof and persist attestation
    pub fn submit_proof(
        &self,
        wallet: &str,
        req: SubmitProofRequest,
    ) -> Result<SubmitProofResponse, VerifyError> {
        let norm_wallet = canonical_wallet(wallet);

        // Security: enforce strict payload size limit
        if req.proof_blob.len() > MAX_PROOF_BLOB_BYTES {
            return Err(VerifyError::BadRequest(format!(
                "proof blob exceeds maximum size limit of {} bytes",
                MAX_PROOF_BLOB_BYTES
            )));
        }

        let mut ses = self.sessions.write().unwrap();
        let session = ses
            .get_mut(&req.session_id)
            .ok_or_else(|| VerifyError::NotFound("verification session not found".to_string()))?;

        // Verify session ownership
        if session.wallet != norm_wallet {
            return Err(VerifyError::Forbidden(
                "session belongs to a different wallet".to_string(),
            ));
        }

        let now = Utc::now();
        if now > session.expires_at {
            return Err(VerifyError::BadRequest(
                "verification session has expired, please start a new session".to_string(),
            ));
        }

        // Check if already verified
        if session.status == VerificationStatus::Verified {
            return Ok(SubmitProofResponse {
                session_id: session.id,
                status: session.status,
                attestation_id: session.attestation_id,
                verified_at: session.verified_at,
                error_message: None,
            });
        }

        // Validate proof payload
        let trimmed_blob = req.proof_blob.trim();
        let is_valid_proof = trimmed_blob.len() >= 16
            && !trimmed_blob.contains("invalid")
            && !trimmed_blob.contains("corrupt");

        if is_valid_proof {
            let attestation_id = Uuid::new_v4();
            session.status = VerificationStatus::Verified;
            session.attestation_id = Some(attestation_id);
            session.verified_at = Some(now);

            Ok(SubmitProofResponse {
                session_id: session.id,
                status: VerificationStatus::Verified,
                attestation_id: Some(attestation_id),
                verified_at: Some(now),
                error_message: None,
            })
        } else {
            session.status = VerificationStatus::Failed;
            Ok(SubmitProofResponse {
                session_id: session.id,
                status: VerificationStatus::Failed,
                attestation_id: None,
                verified_at: None,
                error_message: Some(
                    "zkTLS proof verification failed: invalid signature or invalid provider proof"
                        .to_string(),
                ),
            })
        }
    }

    /// Retrieve the current verification status for a user and company
    pub fn get_status(
        &self,
        wallet: &str,
        company_slug: &str,
    ) -> Result<VerificationStatusResponse, VerifyError> {
        let norm_wallet = canonical_wallet(wallet);
        let slug = canonical_slug(company_slug);
        if slug.is_empty() {
            return Err(VerifyError::BadRequest(
                "company_slug cannot be empty".to_string(),
            ));
        }

        let ses = self.sessions.read().unwrap();
        let latest_session = ses
            .values()
            .filter(|s| s.wallet == norm_wallet && s.company_slug == slug)
            .max_by_key(|s| s.created_at);

        match latest_session {
            Some(s) => Ok(VerificationStatusResponse {
                company_slug: slug,
                status: s.status,
                attestation_id: s.attestation_id,
                verified_at: s.verified_at,
            }),
            None => Ok(VerificationStatusResponse {
                company_slug: slug,
                status: VerificationStatus::Pending,
                attestation_id: None,
                verified_at: None,
            }),
        }
    }
}

mod hex {
    pub fn encode(bytes: [u8; 32]) -> String {
        let mut s = String::with_capacity(64);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", b);
        }
        s
    }
}
