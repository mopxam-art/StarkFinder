use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use chrono::Utc;
use regex::Regex;
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    models::comment::{Comment, CommentNode, CommentStatus, CommentsListRes, CreateCommentDto},
};

pub const MAX_COMMENT_LENGTH: usize = 4000;
pub const MAX_THREAD_DEPTH: usize = 20;
pub const DEFAULT_PAGE_LIMIT: usize = 20;
pub const MAX_PAGE_LIMIT: usize = 100;

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}").unwrap()
});

static PHONE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+?\d{1,3}[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}").unwrap()
});

static PRIVATE_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"0x[a-fA-F0-9]{64}").unwrap()
});

static CREDIT_CARD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:\d{4}[ -]?){3}\d{4}\b").unwrap()
});

/// Thread-safe in-memory comment store
#[derive(Default)]
pub struct CommentStore {
    comments: RwLock<HashMap<String, Comment>>,
}

pub static COMMENT_STORE: LazyLock<CommentStore> = LazyLock::new(CommentStore::default);

impl CommentStore {
    pub fn new() -> Self {
        Self {
            comments: RwLock::new(HashMap::new()),
        }
    }

    /// Clear all comments (used for unit/integration tests)
    pub fn clear(&self) {
        let mut store = self.comments.write().unwrap();
        store.clear();
    }

    /// Create a new comment or nested reply with validation, depth check, and PII sanitization
    pub fn create_comment(
        &self,
        post_id: &str,
        author_wallet: &str,
        dto: CreateCommentDto,
    ) -> Result<CommentNode, ApiError> {
        let trimmed = dto.content.trim();
        if trimmed.is_empty() {
            return Err(ApiError::BadRequest("comment content cannot be empty"));
        }
        if trimmed.len() > MAX_COMMENT_LENGTH {
            return Err(ApiError::BadRequest(
                "comment content exceeds maximum length of 4000 characters",
            ));
        }

        let mut store = self.comments.write().unwrap();

        // Validate parent_id if provided
        let depth = if let Some(ref pid) = dto.parent_id {
            let parent = store
                .get(pid)
                .ok_or(ApiError::NotFound("parent comment not found"))?;

            if parent.post_id != post_id {
                return Err(ApiError::BadRequest(
                    "parent comment does not belong to the specified post",
                ));
            }

            if parent.status == CommentStatus::Deleted {
                return Err(ApiError::Gone(
                    "cannot reply to a deleted comment",
                ));
            }

            if parent.depth >= MAX_THREAD_DEPTH {
                return Err(ApiError::BadRequest(
                    "maximum thread reply depth reached",
                ));
            }

            parent.depth + 1
        } else {
            0
        };

        // Sanitize PII and toxicity
        let sanitized_content = sanitize_comment_content(trimmed);

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let norm_wallet = canonical_wallet(author_wallet);

        let comment = Comment {
            id: id.clone(),
            post_id: post_id.to_string(),
            author_wallet: norm_wallet.clone(),
            parent_id: dto.parent_id.clone(),
            content: sanitized_content.clone(),
            depth,
            status: CommentStatus::Active,
            created_at: now,
            updated_at: now,
        };

        store.insert(id.clone(), comment);

        Ok(CommentNode {
            id,
            post_id: post_id.to_string(),
            author_wallet: norm_wallet,
            parent_id: dto.parent_id,
            content: sanitized_content,
            depth,
            status: CommentStatus::Active,
            reply_count: 0,
            created_at: now,
            updated_at: now,
        })
    }

    /// List comments for a post in flat hierarchical tree order with cursor pagination
    pub fn list_comments(
        &self,
        post_id: &str,
        cursor: Option<&str>,
        limit: Option<usize>,
    ) -> Result<CommentsListRes, ApiError> {
        let store = self.comments.read().unwrap();

        // 1. Filter comments belonging to this post
        let mut post_comments: Vec<Comment> = store
            .values()
            .filter(|c| c.post_id == post_id)
            .cloned()
            .collect();

        let total_count = post_comments.len();

        // 2. Count replies per comment
        let mut reply_counts: HashMap<String, usize> = HashMap::new();
        for c in &post_comments {
            if let Some(ref pid) = c.parent_id {
                *reply_counts.entry(pid.clone()).or_insert(0) += 1;
            }
        }

        // 3. Build hierarchical tree order (top-level chronological, replies nested beneath)
        let tree_sorted = build_threaded_order(&mut post_comments);

        // 4. Cursor-based pagination
        let page_limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
        let start_index = if let Some(c_id) = cursor {
            tree_sorted
                .iter()
                .position(|c| c.id == c_id)
                .map(|idx| idx + 1)
                .unwrap_or(0)
        } else {
            0
        };

        let end_index = (start_index + page_limit).min(tree_sorted.len());
        let paginated_slice = if start_index < tree_sorted.len() {
            &tree_sorted[start_index..end_index]
        } else {
            &[]
        };

        let has_more = end_index < tree_sorted.len();
        let next_cursor = if has_more && !paginated_slice.is_empty() {
            Some(paginated_slice.last().unwrap().id.clone())
        } else {
            None
        };

        let comments = paginated_slice
            .iter()
            .map(|c| {
                let display_content = match c.status {
                    CommentStatus::Active => c.content.clone(),
                    CommentStatus::Deleted => "[deleted]".to_string(),
                    CommentStatus::Moderated => "[moderated by admin]".to_string(),
                };
                let count = reply_counts.get(&c.id).copied().unwrap_or(0);
                CommentNode {
                    id: c.id.clone(),
                    post_id: c.post_id.clone(),
                    author_wallet: c.author_wallet.clone(),
                    parent_id: c.parent_id.clone(),
                    content: display_content,
                    depth: c.depth,
                    status: c.status,
                    reply_count: count,
                    created_at: c.created_at,
                    updated_at: c.updated_at,
                }
            })
            .collect();

        Ok(CommentsListRes {
            comments,
            next_cursor,
            total_count,
            has_more,
        })
    }

    /// Soft delete comment by author or moderator
    pub fn soft_delete_comment(
        &self,
        comment_id: &str,
        requester_wallet: &str,
        is_moderator: bool,
    ) -> Result<CommentNode, ApiError> {
        let mut store = self.comments.write().unwrap();
        let comment = store
            .get_mut(comment_id)
            .ok_or(ApiError::NotFound("comment not found"))?;

        if comment.status == CommentStatus::Deleted {
            return Err(ApiError::Gone("comment has already been deleted"));
        }

        let norm_requester = canonical_wallet(requester_wallet);
        if !is_moderator && comment.author_wallet != norm_requester {
            return Err(ApiError::Forbidden(
                "only author or moderator can delete this comment",
            ));
        }

        comment.status = CommentStatus::Deleted;
        comment.content = "[deleted]".to_string();
        comment.updated_at = Utc::now();

        Ok(CommentNode {
            id: comment.id.clone(),
            post_id: comment.post_id.clone(),
            author_wallet: comment.author_wallet.clone(),
            parent_id: comment.parent_id.clone(),
            content: "[deleted]".to_string(),
            depth: comment.depth,
            status: CommentStatus::Deleted,
            reply_count: 0,
            created_at: comment.created_at,
            updated_at: comment.updated_at,
        })
    }

    /// Moderate comment explicitly by administrator
    pub fn moderate_comment(
        &self,
        comment_id: &str,
    ) -> Result<CommentNode, ApiError> {
        let mut store = self.comments.write().unwrap();
        let comment = store
            .get_mut(comment_id)
            .ok_or(ApiError::NotFound("comment not found"))?;

        comment.status = CommentStatus::Moderated;
        comment.content = "[moderated by admin]".to_string();
        comment.updated_at = Utc::now();

        Ok(CommentNode {
            id: comment.id.clone(),
            post_id: comment.post_id.clone(),
            author_wallet: comment.author_wallet.clone(),
            parent_id: comment.parent_id.clone(),
            content: "[moderated by admin]".to_string(),
            depth: comment.depth,
            status: CommentStatus::Moderated,
            reply_count: 0,
            created_at: comment.created_at,
            updated_at: comment.updated_at,
        })
    }

    /// Retrieve single comment by ID
    pub fn get_comment(&self, comment_id: &str) -> Result<CommentNode, ApiError> {
        let store = self.comments.read().unwrap();
        let c = store
            .get(comment_id)
            .ok_or(ApiError::NotFound("comment not found"))?;

        let display_content = match c.status {
            CommentStatus::Active => c.content.clone(),
            CommentStatus::Deleted => "[deleted]".to_string(),
            CommentStatus::Moderated => "[moderated by admin]".to_string(),
        };

        Ok(CommentNode {
            id: c.id.clone(),
            post_id: c.post_id.clone(),
            author_wallet: c.author_wallet.clone(),
            parent_id: c.parent_id.clone(),
            content: display_content,
            depth: c.depth,
            status: c.status,
            reply_count: 0,
            created_at: c.created_at,
            updated_at: c.updated_at,
        })
    }
}

/// Helper: canonicalize wallet address
pub fn canonical_wallet(wallet: &str) -> String {
    crate::libs::wallet::normalize_and_validate(wallet)
        .unwrap_or_else(|_| wallet.trim().to_lowercase())
}

/// Helper: sanitize PII and toxic content from comments
pub fn sanitize_comment_content(raw: &str) -> String {
    let mut sanitized = raw.to_string();

    // 1. Redact Private keys
    sanitized = PRIVATE_KEY_REGEX.replace_all(&sanitized, "[REDACTED_KEY]").to_string();

    // 2. Redact Credit cards
    sanitized = CREDIT_CARD_REGEX.replace_all(&sanitized, "[REDACTED_CARD]").to_string();

    // 3. Redact Email addresses
    sanitized = EMAIL_REGEX.replace_all(&sanitized, "[REDACTED_EMAIL]").to_string();

    // 4. Redact Phone numbers
    sanitized = PHONE_REGEX.replace_all(&sanitized, "[REDACTED_PHONE]").to_string();

    // 5. Profanity / scam keyword filtering
    let scam_terms = ["phishing link", "drainer wallet", "seed phrase transfer"];
    for term in scam_terms {
        if sanitized.to_lowercase().contains(term) {
            sanitized = sanitized.replace(term, "[FILTERED]");
        }
    }

    sanitized
}

/// Helper: order comments in DFS tree order (parent followed recursively by replies)
pub fn build_threaded_order(comments: &mut [Comment]) -> Vec<Comment> {
    // Sort all comments chronologically first
    comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let mut children_map: HashMap<Option<String>, Vec<Comment>> = HashMap::new();
    for c in comments.iter() {
        children_map
            .entry(c.parent_id.clone())
            .or_default()
            .push(c.clone());
    }

    let mut result = Vec::with_capacity(comments.len());
    if let Some(roots) = children_map.remove(&None) {
        for root in roots {
            traverse_tree(&root, &children_map, &mut result);
        }
    }

    result
}

fn traverse_tree(
    current: &Comment,
    children_map: &HashMap<Option<String>, Vec<Comment>>,
    accumulator: &mut Vec<Comment>,
) {
    accumulator.push(current.clone());
    if let Some(children) = children_map.get(&Some(current.id.clone())) {
        for child in children {
            traverse_tree(child, children_map, accumulator);
        }
    }
}
