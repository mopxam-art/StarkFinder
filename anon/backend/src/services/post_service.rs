use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    libs::error::ErrorBody,
    models::post::{PostFeedResponse, PostStatus, ReviewPost},
};

pub const DEFAULT_LIMIT: usize = 10;
pub const MAX_LIMIT: usize = 50;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum PostError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Gone: {0}")]
    Gone(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for PostError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            PostError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            PostError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            PostError::Gone(m) => (StatusCode::GONE, m.clone()),
            PostError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
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

/// Thread-safe in-memory store for company review posts
pub struct PostService {
    posts: RwLock<HashMap<Uuid, ReviewPost>>,
}

impl Default for PostService {
    fn default() -> Self {
        Self::new()
    }
}

pub static POST_SERVICE: LazyLock<PostService> = LazyLock::new(PostService::default);

impl PostService {
    pub fn new() -> Self {
        Self {
            posts: RwLock::new(HashMap::new()),
        }
    }

    /// Reset all state (useful for test isolation)
    pub fn clear_all(&self) {
        let mut po = self.posts.write().unwrap();
        po.clear();
    }

    /// Insert a review post directly into the store (useful for seeding and creation)
    pub fn insert_post(&self, post: ReviewPost) {
        let mut po = self.posts.write().unwrap();
        po.insert(post.id, post);
    }

    /// Create and persist a new review post
    pub fn create_post(
        &self,
        wallet: &str,
        company_slug: &str,
        text: &str,
        tags: Vec<String>,
        rating: u8,
    ) -> Result<ReviewPost, PostError> {
        let norm_wallet = canonical_wallet(wallet);
        let slug = canonical_slug(company_slug);
        if slug.is_empty() {
            return Err(PostError::BadRequest("company_slug cannot be empty".to_string()));
        }

        let trimmed_text = text.trim();
        if trimmed_text.len() < 5 {
            return Err(PostError::BadRequest("review text too short".to_string()));
        }

        let sanitized_text = sanitize_text(trimmed_text);
        let sanitized_tags: Vec<String> = tags.into_iter().map(|t| t.trim().to_lowercase()).collect();

        let post_id = Uuid::new_v4();
        let post = ReviewPost {
            id: post_id,
            company_slug: slug,
            author_wallet: norm_wallet,
            text: sanitized_text,
            tags: sanitized_tags,
            rating: rating.clamp(1, 5),
            status: PostStatus::Published,
            created_at: Utc::now(),
        };

        let mut po = self.posts.write().unwrap();
        po.insert(post_id, post.clone());
        Ok(post)
    }

    /// Soft-delete an existing review post
    pub fn soft_delete_post(&self, id: Uuid) -> Result<(), PostError> {
        let mut po = self.posts.write().unwrap();
        let post = po
            .get_mut(&id)
            .ok_or_else(|| PostError::NotFound("review post not found".to_string()))?;
        post.status = PostStatus::Deleted;
        Ok(())
    }

    /// Retrieve a single review post by ID (returns 410 Gone if deleted)
    pub fn get_post(&self, id: Uuid) -> Result<ReviewPost, PostError> {
        let po = self.posts.read().unwrap();
        let post = po
            .get(&id)
            .ok_or_else(|| PostError::NotFound("review post not found".to_string()))?;

        if post.status == PostStatus::Deleted {
            return Err(PostError::Gone(
                "this review has been deleted and is no longer available".to_string(),
            ));
        }

        Ok(post.clone())
    }

    /// List review posts for a company feed with cursor pagination and filters
    #[allow(clippy::too_many_arguments)]
    pub fn list_company_posts(
        &self,
        company_slug: &str,
        limit_opt: Option<usize>,
        cursor_opt: Option<String>,
        tag_opt: Option<String>,
        since_opt: Option<DateTime<Utc>>,
        until_opt: Option<DateTime<Utc>>,
        status_opt: Option<PostStatus>,
    ) -> Result<PostFeedResponse, PostError> {
        let slug = canonical_slug(company_slug);
        let target_status = status_opt.unwrap_or(PostStatus::Published);
        let limit = limit_opt.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

        let po = self.posts.read().unwrap();
        let mut list: Vec<ReviewPost> = po
            .values()
            .filter(|p| {
                if p.company_slug != slug {
                    return false;
                }
                if p.status != target_status {
                    return false;
                }
                if let Some(ref tag) = tag_opt {
                    let norm_tag = tag.trim().to_lowercase();
                    if !p.tags.iter().any(|t| t == &norm_tag) {
                        return false;
                    }
                }
                if let Some(since) = since_opt {
                    if p.created_at < since {
                        return false;
                    }
                }
                if let Some(until) = until_opt {
                    if p.created_at > until {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Stable ordering by created_at DESC, id DESC
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));

        let total = list.len();

        let start_idx = if let Some(cursor_str) = cursor_opt {
            let cursor_uuid = Uuid::parse_str(&cursor_str)
                .map_err(|_| PostError::BadRequest("invalid pagination cursor format".to_string()))?;
            match list.iter().position(|p| p.id == cursor_uuid) {
                Some(pos) => pos + 1,
                None => 0,
            }
        } else {
            0
        };

        if start_idx >= total {
            return Ok(PostFeedResponse {
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

        Ok(PostFeedResponse {
            items: page_items,
            total,
            next_cursor,
            has_more,
        })
    }
}
