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
    models::bookmark::{Bookmark, BookmarkResponse, BookmarkStatusResponse},
};

pub const DEFAULT_LIMIT: usize = 10;
pub const MAX_LIMIT: usize = 50;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum BookmarkError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for BookmarkError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            BookmarkError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            BookmarkError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            BookmarkError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            BookmarkError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
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

/// Thread-safe in-memory store for user bookmarks
pub struct BookmarkService {
    bookmarks: RwLock<HashMap<(String, Uuid), Bookmark>>,
}

impl Default for BookmarkService {
    fn default() -> Self {
        Self::new()
    }
}

pub static BOOKMARK_SERVICE: LazyLock<BookmarkService> = LazyLock::new(BookmarkService::default);

impl BookmarkService {
    pub fn new() -> Self {
        Self {
            bookmarks: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut bm = self.bookmarks.write().unwrap();
        bm.clear();
    }

    /// Add a bookmark for a post (idempotent: returns existing if already present)
    pub fn add_bookmark(&self, user_wallet: &str, post_id: Uuid) -> Result<Bookmark, BookmarkError> {
        let norm_wallet = canonical_wallet(user_wallet);
        if norm_wallet.is_empty() {
            return Err(BookmarkError::BadRequest("invalid wallet address".to_string()));
        }

        let key = (norm_wallet.clone(), post_id);
        let mut bm = self.bookmarks.write().unwrap();

        if let Some(existing) = bm.get(&key) {
            return Ok(existing.clone());
        }

        let bookmark = Bookmark {
            id: Uuid::new_v4(),
            user_wallet: norm_wallet,
            post_id,
            created_at: Utc::now(),
        };

        bm.insert(key, bookmark.clone());
        Ok(bookmark)
    }

    /// Remove a bookmark for a post (idempotent)
    pub fn remove_bookmark(&self, user_wallet: &str, post_id: Uuid) -> Result<(), BookmarkError> {
        let norm_wallet = canonical_wallet(user_wallet);
        let key = (norm_wallet, post_id);
        let mut bm = self.bookmarks.write().unwrap();
        bm.remove(&key);
        Ok(())
    }

    /// Check if a post is bookmarked by user
    pub fn is_bookmarked(&self, user_wallet: &str, post_id: Uuid) -> bool {
        let norm_wallet = canonical_wallet(user_wallet);
        let key = (norm_wallet, post_id);
        let bm = self.bookmarks.read().unwrap();
        bm.contains_key(&key)
    }

    /// Get bookmark status response for a post
    pub fn get_bookmark_status(&self, user_wallet: &str, post_id: Uuid) -> BookmarkStatusResponse {
        let is_bookmarked = self.is_bookmarked(user_wallet, post_id);
        BookmarkStatusResponse {
            post_id,
            is_bookmarked,
        }
    }

    /// List bookmarks for a user with cursor pagination
    pub fn list_bookmarks(
        &self,
        user_wallet: &str,
        limit_opt: Option<usize>,
        cursor_opt: Option<String>,
    ) -> Result<BookmarkResponse, BookmarkError> {
        let norm_wallet = canonical_wallet(user_wallet);
        let limit = limit_opt.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

        let bm = self.bookmarks.read().unwrap();
        let mut user_bookmarks: Vec<Bookmark> = bm
            .values()
            .filter(|b| b.user_wallet == norm_wallet)
            .cloned()
            .collect();

        // Sort descending by created_at, then by id for deterministic pagination
        user_bookmarks.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));

        let total = user_bookmarks.len();

        let start_idx = if let Some(cursor_str) = cursor_opt {
            let cursor_uuid = Uuid::parse_str(&cursor_str)
                .map_err(|_| BookmarkError::BadRequest("invalid cursor format".to_string()))?;
            match user_bookmarks.iter().position(|b| b.id == cursor_uuid) {
                Some(pos) => pos + 1,
                None => 0,
            }
        } else {
            0
        };

        if start_idx >= total {
            return Ok(BookmarkResponse {
                items: Vec::new(),
                total,
                next_cursor: None,
                has_more: false,
            });
        }

        let end_idx = (start_idx + limit).min(total);
        let page_items = user_bookmarks[start_idx..end_idx].to_vec();
        let has_more = end_idx < total;
        let next_cursor = if has_more && !page_items.is_empty() {
            Some(page_items.last().unwrap().id.to_string())
        } else {
            None
        };

        Ok(BookmarkResponse {
            items: page_items,
            total,
            next_cursor,
            has_more,
        })
    }
}
