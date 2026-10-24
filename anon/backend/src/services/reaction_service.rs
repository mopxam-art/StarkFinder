use chrono::Utc;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::libs::error::ApiError;
use crate::models::reaction::{
    ReactionRecord, ReactionResponse, ReactionSummary, ReactionTarget, ReactionType,
};

pub fn canonical_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim().to_lowercase();
    if trimmed.starts_with("0x") {
        trimmed
    } else {
        format!("0x{}", trimmed)
    }
}

pub struct ReactionStore {
    // Key: (target_type, target_id, user_wallet)
    reactions: RwLock<HashMap<(ReactionTarget, String, String), ReactionRecord>>,
    // Key: (target_type, target_id) -> author_wallet
    target_authors: RwLock<HashMap<(ReactionTarget, String), String>>,
    // Key: user_wallet -> list of request timestamps
    rate_limits: RwLock<HashMap<String, Vec<Instant>>>,
    // Rate limit configuration: max_requests, window duration
    rate_config: RwLock<(usize, Duration)>,
}

impl Default for ReactionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactionStore {
    pub fn new() -> Self {
        Self {
            reactions: RwLock::new(HashMap::new()),
            target_authors: RwLock::new(HashMap::new()),
            rate_limits: RwLock::new(HashMap::new()),
            rate_config: RwLock::new((50, Duration::from_secs(60))),
        }
    }

    pub fn set_rate_limit(&self, max_requests: usize, window: Duration) {
        let mut cfg = self.rate_config.write().unwrap();
        *cfg = (max_requests, window);
    }

    pub fn set_target_author(
        &self,
        target_type: ReactionTarget,
        target_id: &str,
        author_wallet: &str,
    ) {
        let mut authors = self.target_authors.write().unwrap();
        authors.insert(
            (target_type, target_id.to_string()),
            canonical_wallet(author_wallet),
        );
    }

    pub fn get_target_author(
        &self,
        target_type: ReactionTarget,
        target_id: &str,
    ) -> Option<String> {
        let authors = self.target_authors.read().unwrap();
        authors.get(&(target_type, target_id.to_string())).cloned()
    }

    pub fn check_rate_limit(&self, wallet: &str) -> Result<(), ApiError> {
        let canonical = canonical_wallet(wallet);
        let (max_requests, window) = *self.rate_config.read().unwrap();
        let now = Instant::now();

        let mut limits = self.rate_limits.write().unwrap();
        let timestamps = limits.entry(canonical).or_default();
        timestamps.retain(|&t| now.duration_since(t) < window);

        if timestamps.len() >= max_requests {
            return Err(ApiError::TooManyRequests("rate limit exceeded"));
        }

        timestamps.push(now);
        Ok(())
    }

    pub fn reset_rate_limits(&self) {
        let mut limits = self.rate_limits.write().unwrap();
        limits.clear();
    }

    pub fn clear(&self) {
        self.reactions.write().unwrap().clear();
        self.target_authors.write().unwrap().clear();
        self.rate_limits.write().unwrap().clear();
        *self.rate_config.write().unwrap() = (50, Duration::from_secs(60));
    }

    pub fn record_reaction(
        &self,
        user_wallet: &str,
        target_type: ReactionTarget,
        target_id: &str,
        reaction_type: ReactionType,
    ) -> Result<ReactionResponse, ApiError> {
        let canonical_user = canonical_wallet(user_wallet);

        // D4: Anti-Abuse - Check if user is attempting to react to their own content
        {
            let authors = self.target_authors.read().unwrap();
            if let Some(author) = authors.get(&(target_type, target_id.to_string())) {
                if author == &canonical_user {
                    return Err(ApiError::Forbidden("cannot react to own content"));
                }
            }
        }

        // D4: Rate limiting
        self.check_rate_limit(&canonical_user)?;

        let now = Utc::now();
        let key = (target_type, target_id.to_string(), canonical_user.clone());

        let mut reactions = self.reactions.write().unwrap();
        let status = if let Some(existing) = reactions.get_mut(&key) {
            if existing.reaction_type == reaction_type {
                // Idempotent: same reaction type, no counter change
                "unchanged".to_string()
            } else {
                // Switched reaction type: update atomically
                existing.reaction_type = reaction_type;
                existing.updated_at = now;
                "switched".to_string()
            }
        } else {
            // New reaction recorded
            let record = ReactionRecord {
                id: Uuid::new_v4().to_string(),
                user_id: canonical_user.clone(),
                target_type,
                target_id: target_id.to_string(),
                reaction_type,
                created_at: now,
                updated_at: now,
            };
            reactions.insert(key, record);
            "recorded".to_string()
        };

        // Compute summary directly from store
        let summary = self.compute_summary_locked(
            &reactions,
            Some(&canonical_user),
            target_type,
            target_id,
        );

        Ok(ReactionResponse {
            target_type: target_type.as_str().to_string(),
            target_id: target_id.to_string(),
            status,
            summary,
        })
    }

    pub fn remove_reaction(
        &self,
        user_wallet: &str,
        target_type: ReactionTarget,
        target_id: &str,
    ) -> Result<ReactionResponse, ApiError> {
        let canonical_user = canonical_wallet(user_wallet);
        let key = (target_type, target_id.to_string(), canonical_user.clone());

        let mut reactions = self.reactions.write().unwrap();
        let status = if reactions.remove(&key).is_some() {
            "removed".to_string()
        } else {
            "unchanged".to_string()
        };

        let summary = self.compute_summary_locked(
            &reactions,
            Some(&canonical_user),
            target_type,
            target_id,
        );

        Ok(ReactionResponse {
            target_type: target_type.as_str().to_string(),
            target_id: target_id.to_string(),
            status,
            summary,
        })
    }

    pub fn get_summary(
        &self,
        user_wallet: Option<&str>,
        target_type: ReactionTarget,
        target_id: &str,
    ) -> ReactionSummary {
        let reactions = self.reactions.read().unwrap();
        let canonical_user = user_wallet.map(canonical_wallet);
        self.compute_summary_locked(
            &reactions,
            canonical_user.as_deref(),
            target_type,
            target_id,
        )
    }

    fn compute_summary_locked(
        &self,
        reactions: &HashMap<(ReactionTarget, String, String), ReactionRecord>,
        user_wallet: Option<&str>,
        target_type: ReactionTarget,
        target_id: &str,
    ) -> ReactionSummary {
        let mut upvotes = 0u64;
        let mut insightful = 0u64;
        let mut thanks = 0u64;
        let mut user_reaction = None;

        for ((r_target, r_id, r_user), record) in reactions.iter() {
            if *r_target == target_type && r_id == target_id {
                match record.reaction_type {
                    ReactionType::Upvote => upvotes += 1,
                    ReactionType::Insightful => insightful += 1,
                    ReactionType::Thanks => thanks += 1,
                }
                if let Some(uw) = user_wallet {
                    if r_user == uw {
                        user_reaction = Some(record.reaction_type);
                    }
                }
            }
        }

        ReactionSummary {
            upvotes,
            insightful,
            thanks,
            total: upvotes + insightful + thanks,
            user_reaction,
        }
    }
}

pub static REACTION_STORE: LazyLock<ReactionStore> = LazyLock::new(ReactionStore::new);
