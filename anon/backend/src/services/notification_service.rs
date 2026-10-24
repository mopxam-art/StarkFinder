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
    models::notification::{
        Follow, FollowKind, Notification, NotificationType, PaginatedResponse,
    },
};

pub const DEFAULT_PAGE_LIMIT: usize = 20;
pub const MAX_PAGE_LIMIT: usize = 100;
pub const MAX_TARGET_ID_LENGTH: usize = 128;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum NotificationError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Not following target: {0}")]
    NotFollowing(String),
    #[error("Already following target: {0}")]
    AlreadyFollowing(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal server error: {0}")]
    Internal(String),
}

impl IntoResponse for NotificationError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            NotificationError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            NotificationError::NotFollowing(m) => (StatusCode::NOT_FOUND, m.clone()),
            NotificationError::AlreadyFollowing(m) => (StatusCode::CONFLICT, m.clone()),
            NotificationError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            NotificationError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            NotificationError::InvalidInput(m) => (StatusCode::BAD_REQUEST, m.clone()),
            NotificationError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

/// Key identifying a unique subscription
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FollowKey {
    user_id: String,
    kind: FollowKind,
    target_id: String,
}

/// In-memory thread-safe store for subscriptions and in-app alerts
pub struct NotificationStore {
    follows: RwLock<HashMap<FollowKey, Follow>>,
    notifications: RwLock<HashMap<Uuid, Notification>>,
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self::new()
    }
}

pub static NOTIFICATION_STORE: LazyLock<NotificationStore> =
    LazyLock::new(NotificationStore::default);

impl NotificationStore {
    pub fn new() -> Self {
        Self {
            follows: RwLock::new(HashMap::new()),
            notifications: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear(&self) {
        let mut follows = self.follows.write().unwrap();
        follows.clear();
        let mut notifications = self.notifications.write().unwrap();
        notifications.clear();
    }

    /// Subscribe user to target company or tag (idempotent: does not duplicate on repeat calls)
    pub fn follow_target(
        &self,
        user_id: &str,
        kind: FollowKind,
        target_id: &str,
    ) -> Result<Follow, NotificationError> {
        let norm_user = canonical_user(user_id);
        let norm_target = canonical_target(target_id)?;

        let key = FollowKey {
            user_id: norm_user.clone(),
            kind,
            target_id: norm_target.clone(),
        };

        let mut follows = self.follows.write().unwrap();
        if let Some(existing) = follows.get(&key) {
            // Idempotent: return existing follow without creating duplicate
            return Ok(existing.clone());
        }

        let follow = Follow {
            follower_user_id: norm_user,
            kind,
            target_id: norm_target,
            created_at: Utc::now(),
        };

        follows.insert(key, follow.clone());
        Ok(follow)
    }

    /// Unsubscribe user from target company or tag
    pub fn unfollow_target(
        &self,
        user_id: &str,
        kind: FollowKind,
        target_id: &str,
    ) -> Result<(), NotificationError> {
        let norm_user = canonical_user(user_id);
        let norm_target = canonical_target(target_id)?;

        let key = FollowKey {
            user_id: norm_user,
            kind,
            target_id: norm_target.clone(),
        };

        let mut follows = self.follows.write().unwrap();
        if follows.remove(&key).is_some() {
            Ok(())
        } else {
            Err(NotificationError::NotFollowing(format!(
                "You are not following {} '{}'",
                kind, norm_target
            )))
        }
    }

    /// Check if user is actively following a target
    pub fn is_following(&self, user_id: &str, kind: FollowKind, target_id: &str) -> bool {
        let norm_user = canonical_user(user_id);
        let Ok(norm_target) = canonical_target(target_id) else {
            return false;
        };

        let key = FollowKey {
            user_id: norm_user,
            kind,
            target_id: norm_target,
        };

        let follows = self.follows.read().unwrap();
        follows.contains_key(&key)
    }

    /// List unique follower user IDs for a given target
    pub fn list_followers(&self, kind: FollowKind, target_id: &str) -> Vec<String> {
        let Ok(norm_target) = canonical_target(target_id) else {
            return Vec::new();
        };

        let follows = self.follows.read().unwrap();
        let mut followers: Vec<String> = follows
            .iter()
            .filter(|(k, _)| k.kind == kind && k.target_id == norm_target)
            .map(|(k, _)| k.user_id.clone())
            .collect();

        followers.sort();
        followers.dedup();
        followers
    }

    /// List all targets followed by a user
    pub fn list_user_follows(&self, user_id: &str, kind: Option<FollowKind>) -> Vec<Follow> {
        let norm_user = canonical_user(user_id);
        let follows = self.follows.read().unwrap();

        let mut list: Vec<Follow> = follows
            .iter()
            .filter(|(k, _)| {
                k.user_id == norm_user && (kind.is_none() || Some(k.kind) == kind)
            })
            .map(|(_, f)| f.clone())
            .collect();

        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    /// Dispatch privacy-preserving in-app alerts to a list of target users
    pub fn dispatch_notification(
        &self,
        target_users: &[String],
        notification_type: NotificationType,
        payload: serde_json::Value,
    ) -> Result<usize, NotificationError> {
        if target_users.is_empty() {
            return Ok(0);
        }

        // Deduplicate target recipients
        let mut unique_recipients = HashSet::new();
        for u in target_users {
            let canon = canonical_user(u);
            if !canon.is_empty() {
                unique_recipients.insert(canon);
            }
        }

        let mut count = 0;
        let mut notifications = self.notifications.write().unwrap();
        let now = Utc::now();

        for recipient in unique_recipients {
            let id = Uuid::new_v4();
            let notif = Notification {
                id,
                user_id: recipient,
                notification_type,
                payload: payload.clone(),
                read_at: None,
                created_at: now,
            };
            notifications.insert(id, notif);
            count += 1;
        }

        Ok(count)
    }

    /// Fetch cursor-paginated in-app notifications strictly for the calling user (privacy-preserving)
    pub fn get_user_notifications(
        &self,
        user_id: &str,
        cursor: Option<&str>,
        limit: Option<usize>,
        only_unread: bool,
    ) -> PaginatedResponse<Notification> {
        let norm_user = canonical_user(user_id);
        let notifications = self.notifications.read().unwrap();

        // 1. Filter strictly for this user (privacy guarantee)
        let mut user_notifs: Vec<Notification> = notifications
            .values()
            .filter(|n| n.user_id == norm_user)
            .cloned()
            .collect();

        // Calculate unread count for user across all their notifications
        let unread_count = user_notifs.iter().filter(|n| n.read_at.is_none()).count();

        // 2. If only_unread requested, filter
        if only_unread {
            user_notifs.retain(|n| n.read_at.is_none());
        }

        let total_count = user_notifs.len();

        // 3. Sort chronologically: newest first (created_at DESC, then id DESC)
        user_notifs.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });

        // 4. Cursor pagination
        let page_limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
        let start_index = if let Some(c_str) = cursor {
            user_notifs
                .iter()
                .position(|n| n.id.to_string() == c_str)
                .map(|idx| idx + 1)
                .unwrap_or(0)
        } else {
            0
        };

        let end_index = (start_index + page_limit).min(user_notifs.len());
        let paginated_slice = if start_index < user_notifs.len() {
            &user_notifs[start_index..end_index]
        } else {
            &[]
        };

        let has_more = end_index < user_notifs.len();
        let next_cursor = if has_more && !paginated_slice.is_empty() {
            Some(paginated_slice.last().unwrap().id.to_string())
        } else {
            None
        };

        PaginatedResponse {
            items: paginated_slice.to_vec(),
            next_cursor,
            total_count,
            unread_count,
            has_more,
        }
    }

    /// Mark a single notification as read with strict ownership validation
    pub fn mark_as_read(
        &self,
        user_id: &str,
        notification_id: Uuid,
    ) -> Result<Notification, NotificationError> {
        let norm_user = canonical_user(user_id);
        let mut notifications = self.notifications.write().unwrap();

        let notif = notifications
            .get_mut(&notification_id)
            .ok_or_else(|| NotificationError::NotFound("Notification not found".to_string()))?;

        // Privacy check: users can only mark their own notifications
        if notif.user_id != norm_user {
            return Err(NotificationError::Forbidden(
                "Access denied: cannot modify other user's notification".to_string(),
            ));
        }

        if notif.read_at.is_none() {
            notif.read_at = Some(Utc::now());
        }

        Ok(notif.clone())
    }

    /// Mark all unread notifications for a user as read
    pub fn mark_all_as_read(&self, user_id: &str) -> Result<usize, NotificationError> {
        let norm_user = canonical_user(user_id);
        let mut notifications = self.notifications.write().unwrap();
        let now = Utc::now();
        let mut count = 0;

        for notif in notifications.values_mut() {
            if notif.user_id == norm_user && notif.read_at.is_none() {
                notif.read_at = Some(now);
                count += 1;
            }
        }

        Ok(count)
    }

    /// Retrieve single notification by ID (with user check)
    pub fn get_notification(
        &self,
        user_id: &str,
        notification_id: Uuid,
    ) -> Result<Notification, NotificationError> {
        let norm_user = canonical_user(user_id);
        let notifications = self.notifications.read().unwrap();

        let notif = notifications
            .get(&notification_id)
            .ok_or_else(|| NotificationError::NotFound("Notification not found".to_string()))?;

        if notif.user_id != norm_user {
            return Err(NotificationError::Forbidden(
                "Access denied: cannot view other user's notification".to_string(),
            ));
        }

        Ok(notif.clone())
    }
}

/// Helper: canonicalize user ID / wallet address
pub fn canonical_user(user: &str) -> String {
    let trimmed = user.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Attempt starknet felt normalization if it looks like a hex address
    crate::libs::wallet::normalize_and_validate(trimmed)
        .unwrap_or_else(|_| trimmed.to_lowercase())
}

/// Helper: sanitize and validate target ID (slug or tag name)
pub fn canonical_target(target: &str) -> Result<String, NotificationError> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err(NotificationError::InvalidInput(
            "Target identifier cannot be empty".to_string(),
        ));
    }
    if trimmed.len() > MAX_TARGET_ID_LENGTH {
        return Err(NotificationError::InvalidInput(format!(
            "Target identifier exceeds maximum length of {} characters",
            MAX_TARGET_ID_LENGTH
        )));
    }
    // Disallow control characters
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(NotificationError::InvalidInput(
            "Target identifier contains invalid characters".to_string(),
        ));
    }
    Ok(trimmed.to_lowercase())
}
