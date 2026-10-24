use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header},
};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    libs::{db::AppState, error::ApiError, jwt},
    middlewares::auth::AuthUser,
};

/// Public Session representation returned by GET /me/sessions
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct SessionItem {
    pub id: String,
    pub wallet: String,
    pub device_name: String,
    pub ip_hash: String,
    pub is_current: bool,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Request body for POST /auth/refresh
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct RefreshReq {
    pub refresh_token: String,
}

/// Response body for POST /auth/refresh
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RefreshRes {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: usize,
    pub session_id: String,
}

/// Request body for POST /auth/logout
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct LogoutReq {
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Response body for POST /auth/logout
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogoutRes {
    pub message: String,
    pub revoked_session_id: Option<String>,
}

/// Response body for DELETE /me/sessions/{id}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevokeSessionRes {
    pub message: String,
    pub session_id: String,
}

/// Internal session record stored in state
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub wallet: String,
    pub current_refresh_token_hash: String,
    pub rotated_token_hashes: Vec<String>,
    pub device_name: String,
    pub ip_hash: String,
    pub is_revoked: bool,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Thread-safe in-memory session store with token rotation and reuse detection
#[derive(Default)]
pub struct SessionStore {
    sessions: RwLock<HashMap<String, SessionRecord>>,
}

pub static SESSION_STORE: LazyLock<SessionStore> = LazyLock::new(SessionStore::default);

impl SessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for tests)
    pub fn clear(&self) {
        let mut store = self.sessions.write().unwrap();
        store.clear();
    }

    /// Create and register a brand new active session
    pub fn create_session(
        &self,
        wallet: &str,
        device_name: &str,
        ip_hash: &str,
        ttl_days: i64,
    ) -> (SessionRecord, String) {
        let session_id = Uuid::new_v4().to_string();
        let raw_refresh_token = generate_secure_token();
        let token_hash = hash_token(&raw_refresh_token);

        let now = Utc::now();
        let expires_at = now + Duration::days(ttl_days);

        let record = SessionRecord {
            id: session_id.clone(),
            wallet: canonical_wallet(wallet),
            current_refresh_token_hash: token_hash,
            rotated_token_hashes: Vec::new(),
            device_name: device_name.to_string(),
            ip_hash: ip_hash.to_string(),
            is_revoked: false,
            created_at: now,
            last_active_at: now,
            expires_at,
        };

        let mut store = self.sessions.write().unwrap();
        store.insert(session_id, record.clone());

        (record, raw_refresh_token)
    }

    /// Rotate refresh token atomically with token reuse detection
    pub fn rotate_token(
        &self,
        raw_refresh_token: &str,
        device_name: Option<&str>,
        ip_hash: Option<&str>,
    ) -> Result<(SessionRecord, String, String), ApiError> {
        let incoming_hash = hash_token(raw_refresh_token);
        let mut store = self.sessions.write().unwrap();

        // 1. Check for token reuse across all rotated tokens
        for (_, session) in store.iter_mut() {
            if session.rotated_token_hashes.contains(&incoming_hash) {
                // TOKEN REUSE ATTACK DETECTED!
                // Per RFC 6749 and OAuth 2.0 Security BCP: immediately revoke all sessions for this wallet
                let compromised_wallet = session.wallet.clone();
                for s in store.values_mut() {
                    if s.wallet == compromised_wallet {
                        s.is_revoked = true;
                    }
                }
                return Err(ApiError::Unauthorized(
                    "token reuse detected: all user sessions revoked for security",
                ));
            }
        }

        // 2. Locate active session matching current_refresh_token_hash
        let mut target_session_id = None;
        for (id, session) in store.iter() {
            if session.current_refresh_token_hash == incoming_hash {
                target_session_id = Some(id.clone());
                break;
            }
        }

        let session_id = target_session_id.ok_or(ApiError::Unauthorized("invalid refresh token"))?;
        let session = store.get_mut(&session_id).unwrap();

        if session.is_revoked {
            return Err(ApiError::Unauthorized("session is revoked"));
        }

        let now = Utc::now();
        if session.expires_at < now {
            session.is_revoked = true;
            return Err(ApiError::Unauthorized("session expired"));
        }

        // 3. Perform atomic rotation
        let old_hash = session.current_refresh_token_hash.clone();
        session.rotated_token_hashes.push(old_hash);

        let new_raw_refresh_token = generate_secure_token();
        session.current_refresh_token_hash = hash_token(&new_raw_refresh_token);
        session.last_active_at = now;

        if let Some(dev) = device_name {
            if !dev.trim().is_empty() {
                session.device_name = dev.to_string();
            }
        }
        if let Some(ip) = ip_hash {
            if !ip.trim().is_empty() {
                session.ip_hash = ip.to_string();
            }
        }

        let jwt_secret = jwt::secret_from_env();
        let access_token = jwt::encode(&session.wallet, &jwt_secret)
            .map_err(|_| ApiError::Internal("failed to generate access token"))?;

        Ok((session.clone(), access_token, new_raw_refresh_token))
    }

    /// Invalidate a session explicitly
    pub fn logout(
        &self,
        raw_refresh_token: Option<&str>,
        session_id: Option<&str>,
        auth_wallet: Option<&str>,
    ) -> Result<Option<String>, ApiError> {
        let mut store = self.sessions.write().unwrap();

        if let Some(token) = raw_refresh_token {
            let hash = hash_token(token);
            for (id, session) in store.iter_mut() {
                if session.current_refresh_token_hash == hash {
                    session.is_revoked = true;
                    return Ok(Some(id.clone()));
                }
            }
        }

        if let Some(sid) = session_id {
            if let Some(session) = store.get_mut(sid) {
                if let Some(w) = auth_wallet {
                    let norm_w = canonical_wallet(w);
                    if session.wallet != norm_w {
                        return Err(ApiError::Forbidden("cannot logout session of another user"));
                    }
                }
                session.is_revoked = true;
                return Ok(Some(sid.to_string()));
            }
        }

        if let Some(w) = auth_wallet {
            let norm_w = canonical_wallet(w);
            // Logout most recently active session for authenticated user
            let mut latest_sid = None;
            let mut latest_time = DateTime::<Utc>::MIN_UTC;
            for (id, session) in store.iter_mut() {
                if session.wallet == norm_w && !session.is_revoked && session.last_active_at > latest_time {
                    latest_time = session.last_active_at;
                    latest_sid = Some(id.clone());
                }
            }
            if let Some(sid) = latest_sid {
                store.get_mut(&sid).unwrap().is_revoked = true;
                return Ok(Some(sid));
            }
        }

        Ok(None)
    }

    /// List all active sessions for a user
    pub fn list_active_sessions(&self, wallet: &str, current_session_id: Option<&str>) -> Vec<SessionItem> {
        let store = self.sessions.read().unwrap();
        let now = Utc::now();
        let norm_wallet = canonical_wallet(wallet);

        let mut results: Vec<SessionItem> = store
            .values()
            .filter(|s| s.wallet == norm_wallet && !s.is_revoked && s.expires_at >= now)
            .map(|s| SessionItem {
                id: s.id.clone(),
                wallet: s.wallet.clone(),
                device_name: s.device_name.clone(),
                ip_hash: s.ip_hash.clone(),
                is_current: current_session_id.map(|cid| cid == s.id).unwrap_or(false),
                created_at: s.created_at,
                last_active_at: s.last_active_at,
                expires_at: s.expires_at,
            })
            .collect();

        results.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
        results
    }

    /// Revoke a specific session belonging to user
    pub fn revoke_session(&self, wallet: &str, session_id: &str) -> Result<(), ApiError> {
        let mut store = self.sessions.write().unwrap();
        let session = store
            .get_mut(session_id)
            .ok_or(ApiError::NotFound("session not found"))?;

        let norm_wallet = canonical_wallet(wallet);
        if session.wallet != norm_wallet {
            return Err(ApiError::Forbidden(
                "forbidden: session belongs to another user",
            ));
        }

        session.is_revoked = true;
        Ok(())
    }

    /// Revoke all active sessions for a user
    pub fn revoke_all_sessions(&self, wallet: &str) {
        let mut store = self.sessions.write().unwrap();
        let norm_wallet = canonical_wallet(wallet);
        for session in store.values_mut() {
            if session.wallet == norm_wallet {
                session.is_revoked = true;
            }
        }
    }
}

/// Helper: canonicalize wallet address
pub fn canonical_wallet(wallet: &str) -> String {
    crate::libs::wallet::normalize_and_validate(wallet).unwrap_or_else(|_| wallet.trim().to_lowercase())
}

/// Helper: generate a cryptographically secure random token (64 hex characters)
pub fn generate_secure_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Helper: compute SHA-256 hash of a string
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Helper: anonymize client IP using salted SHA-256 hash
pub fn hash_ip(ip: &str) -> String {
    let salt = std::env::var("IP_SALT").unwrap_or_else(|_| "starkfinder-ip-salt-v1".to_string());
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(ip.as_bytes());
    let full = hex::encode(hasher.finalize());
    full[..16].to_string()
}

/// Helper: extract client IP and user agent info from HTTP headers
pub fn extract_client_metadata(headers: &HeaderMap) -> (String, String) {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("Unknown Browser");

    let device_name = parse_device_info(user_agent);

    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
        })
        .unwrap_or("127.0.0.1")
        .trim();

    let ip_hash = hash_ip(ip);

    (device_name, ip_hash)
}

/// Helper: simplify User-Agent into a friendly name
pub fn parse_device_info(ua: &str) -> String {
    let ua_lower = ua.to_lowercase();
    let browser = if ua_lower.contains("edg/") {
        "Edge"
    } else if ua_lower.contains("chrome") && !ua_lower.contains("chromium") {
        "Chrome"
    } else if ua_lower.contains("firefox") {
        "Firefox"
    } else if ua_lower.contains("safari") && !ua_lower.contains("chrome") {
        "Safari"
    } else if ua_lower.contains("curl") {
        "cURL Client"
    } else {
        "Web Browser"
    };

    let os = if ua_lower.contains("iphone") || ua_lower.contains("ipad") {
        "iOS"
    } else if ua_lower.contains("macintosh") || ua_lower.contains("mac os x") {
        "macOS"
    } else if ua_lower.contains("windows") {
        "Windows"
    } else if ua_lower.contains("android") {
        "Android"
    } else if ua_lower.contains("linux") {
        "Linux"
    } else {
        "Unknown OS"
    };

    format!("{} ({})", browser, os)
}

// -------------------------------------------------------------------------
// AXUM HTTP HANDLERS
// -------------------------------------------------------------------------

/// POST /auth/logout
/// Terminate current session and invalidate refresh token
#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    request_body(content = Option<LogoutReq>, description = "Optional refresh token or session id to logout"),
    responses(
        (status = 200, description = "Logged out successfully", body = LogoutRes),
        (status = 403, description = "Forbidden: cannot logout session of another user", body = crate::libs::error::ErrorBody),
        (status = 500, description = "Internal error", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn logout(
    headers: HeaderMap,
    Json(payload): Json<Option<LogoutReq>>,
) -> Result<Json<LogoutRes>, ApiError> {
    let req = payload.unwrap_or_default();

    // Check optional auth header if provided
    let auth_wallet = if let Some(auth_hdr) = headers.get(header::AUTHORIZATION) {
        if let Ok(val) = auth_hdr.to_str() {
            if let Some(token) = val.strip_prefix("Bearer ").or_else(|| val.strip_prefix("bearer ")) {
                let secret = jwt::secret_from_env();
                jwt::decode(token, &secret).ok().map(|c| c.sub)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let revoked_id = SESSION_STORE.logout(
        req.refresh_token.as_deref(),
        req.session_id.as_deref(),
        auth_wallet.as_deref(),
    )?;

    Ok(Json(LogoutRes {
        message: "logged out successfully".to_string(),
        revoked_session_id: revoked_id,
    }))
}

/// POST /auth/refresh
/// Atomic rotation of refresh token with Token Reuse Detection
#[utoipa::path(
    post,
    path = "/auth/refresh",
    tag = "auth",
    request_body = RefreshReq,
    responses(
        (status = 200, description = "Tokens rotated successfully", body = RefreshRes),
        (status = 401, description = "Invalid or reused refresh token", body = crate::libs::error::ErrorBody),
        (status = 500, description = "Internal error", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn refresh(
    headers: HeaderMap,
    Json(payload): Json<RefreshReq>,
) -> Result<Json<RefreshRes>, ApiError> {
    let (device_name, ip_hash) = extract_client_metadata(&headers);

    let (session, access_token, new_refresh_token) = SESSION_STORE.rotate_token(
        &payload.refresh_token,
        Some(&device_name),
        Some(&ip_hash),
    )?;

    Ok(Json(RefreshRes {
        access_token,
        refresh_token: new_refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 60 * 60 * 24 * 7, // 7 days in seconds
        session_id: session.id,
    }))
}

/// GET /me/sessions
/// List active sessions for the authenticated user
#[utoipa::path(
    get,
    path = "/me/sessions",
    tag = "auth",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of active user sessions", body = Vec<SessionItem>),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 500, description = "Internal error", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn list_sessions(
    _state: State<AppState>,
    AuthUser { wallet }: AuthUser,
) -> Result<Json<Vec<SessionItem>>, ApiError> {
    let sessions = SESSION_STORE.list_active_sessions(&wallet, None);
    Ok(Json(sessions))
}

/// DELETE /me/sessions/{session_id}
/// Remote revocation of a specific session
#[utoipa::path(
    delete,
    path = "/me/sessions/{session_id}",
    tag = "auth",
    security(("bearer_auth" = [])),
    params(
        ("session_id" = String, Path, description = "UUID of session to revoke")
    ),
    responses(
        (status = 200, description = "Session revoked successfully", body = RevokeSessionRes),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden: session belongs to another user", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Session not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn revoke_session(
    _state: State<AppState>,
    AuthUser { wallet }: AuthUser,
    Path(session_id): Path<String>,
) -> Result<Json<RevokeSessionRes>, ApiError> {
    SESSION_STORE.revoke_session(&wallet, &session_id)?;

    Ok(Json(RevokeSessionRes {
        message: "session revoked successfully".to_string(),
        session_id,
    }))
}
