use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::Instant;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};

use crate::{
    libs::error::ErrorBody,
    models::company_stats::{CacheMetrics, CompanyReview, CompanyStats, ReviewStatus},
};

pub const DEFAULT_TTL_SECONDS: u64 = 60;
pub const MAX_SLUG_LENGTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Hit,
    Miss,
}

impl CacheStatus {
    pub fn as_header_str(&self) -> &'static str {
        match self {
            CacheStatus::Hit => "HIT",
            CacheStatus::Miss => "MISS",
        }
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum StatsError {
    #[error("Company not found: {0}")]
    CompanyNotFound(String),
    #[error("Invalid slug: {0}")]
    InvalidSlug(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for StatsError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            StatsError::CompanyNotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            StatsError::InvalidSlug(m) => (StatusCode::BAD_REQUEST, m.clone()),
            StatsError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

/// Cached statistics entry with expiration
#[derive(Debug, Clone)]
pub struct StatsCacheEntry {
    pub data: CompanyStats,
    pub cached_at: Instant,
    pub ttl_seconds: u64,
}

/// Thread-safe in-memory stats aggregation and caching service
pub struct StatsService {
    known_companies: RwLock<HashSet<String>>,
    reviews: RwLock<HashMap<String, Vec<CompanyReview>>>,
    cache: RwLock<HashMap<String, StatsCacheEntry>>,
    custom_ttls: RwLock<HashMap<String, u64>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl Default for StatsService {
    fn default() -> Self {
        Self::new()
    }
}

pub static STATS_SERVICE: LazyLock<StatsService> = LazyLock::new(StatsService::default);

impl StatsService {
    pub fn new() -> Self {
        let mut known = HashSet::new();
        // Seed default Starknet ecosystem companies
        for s in &[
            "shonen-labs",
            "argent",
            "braavos",
            "avnu",
            "ekubo",
            "starknet-id",
            "fibrous",
            "nostra",
            "zklend",
            "carmine",
        ] {
            known.insert(s.to_string());
        }

        Self {
            known_companies: RwLock::new(known),
            reviews: RwLock::new(HashMap::new()),
            cache: RwLock::new(HashMap::new()),
            custom_ttls: RwLock::new(HashMap::new()),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Reset all state and metrics (useful for test isolation)
    pub fn clear_all(&self) {
        let mut c = self.cache.write().unwrap();
        c.clear();
        let mut r = self.reviews.write().unwrap();
        r.clear();
        let mut ttls = self.custom_ttls.write().unwrap();
        ttls.clear();
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }

    /// Register a company slug as recognized
    pub fn register_company(&self, slug: &str) -> Result<String, StatsError> {
        let norm = canonical_slug(slug)?;
        let mut k = self.known_companies.write().unwrap();
        k.insert(norm.clone());
        Ok(norm)
    }

    /// Check if company slug exists
    pub fn company_exists(&self, slug: &str) -> bool {
        let Ok(norm) = canonical_slug(slug) else {
            return false;
        };
        let k = self.known_companies.read().unwrap();
        if k.contains(&norm) {
            return true;
        }
        let r = self.reviews.read().unwrap();
        r.contains_key(&norm)
    }

    /// Set custom TTL in seconds for a specific company slug (for fast TTL expiration testing)
    pub fn set_custom_ttl(&self, slug: &str, ttl_seconds: u64) -> Result<(), StatsError> {
        let norm = canonical_slug(slug)?;
        let mut ttls = self.custom_ttls.write().unwrap();
        ttls.insert(norm, ttl_seconds);
        Ok(())
    }

    /// Add a review for a company (automatically invalidates cache for that company)
    pub fn add_review(
        &self,
        slug: &str,
        rating: u8,
        status: ReviewStatus,
        toxicity_score: Option<f64>,
        topics: Vec<String>,
        created_at: Option<DateTime<Utc>>,
    ) -> Result<CompanyReview, StatsError> {
        let norm = canonical_slug(slug)?;

        // Ensure company is registered
        {
            let mut k = self.known_companies.write().unwrap();
            k.insert(norm.clone());
        }

        let review = CompanyReview {
            id: format!("rev-{}", rand::random::<u64>()),
            company_slug: norm.clone(),
            rating: rating.clamp(1, 5),
            status,
            toxicity_score,
            topics: topics.into_iter().map(|t| t.trim().to_lowercase()).collect(),
            created_at: created_at.unwrap_or_else(Utc::now),
        };

        {
            let mut r = self.reviews.write().unwrap();
            r.entry(norm.clone()).or_default().push(review.clone());
        }

        // Invalidate cached statistics upon new/updated review
        self.invalidate_cache(&norm);

        Ok(review)
    }

    /// Get company stats returning both aggregate data and cache status (HIT vs MISS)
    pub fn get_company_stats_with_status(
        &self,
        slug: &str,
    ) -> Result<(CompanyStats, CacheStatus), StatsError> {
        let norm = canonical_slug(slug)?;

        if !self.company_exists(&norm) {
            return Err(StatsError::CompanyNotFound(format!(
                "Company '{}' does not exist",
                norm
            )));
        }

        // 1. Check cache for valid entry
        {
            let cache = self.cache.read().unwrap();
            if let Some(entry) = cache.get(&norm) {
                if entry.cached_at.elapsed().as_secs() < entry.ttl_seconds {
                    self.hits.fetch_add(1, Ordering::Relaxed);
                    return Ok((entry.data.clone(), CacheStatus::Hit));
                }
            }
        }

        // 2. Cache miss -> compute aggregate stats
        self.misses.fetch_add(1, Ordering::Relaxed);
        let stats = self.compute_aggregates(&norm)?;

        let ttl = {
            let ttls = self.custom_ttls.read().unwrap();
            ttls.get(&norm).copied().unwrap_or(DEFAULT_TTL_SECONDS)
        };

        // 3. Store in cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(
                norm,
                StatsCacheEntry {
                    data: stats.clone(),
                    cached_at: Instant::now(),
                    ttl_seconds: ttl,
                },
            );
        }

        Ok((stats, CacheStatus::Miss))
    }

    /// Compute raw statistics aggregates for a company
    fn compute_aggregates(&self, norm_slug: &str) -> Result<CompanyStats, StatsError> {
        let r = self.reviews.read().unwrap();
        let company_reviews = r.get(norm_slug).cloned().unwrap_or_default();

        let total_reviews = company_reviews.len() as u64;

        // Filter published reviews for rating and toxicity aggregates
        let published_reviews_vec: Vec<&CompanyReview> = company_reviews
            .iter()
            .filter(|rev| rev.status == ReviewStatus::Published)
            .collect();

        let published_reviews = published_reviews_vec.len() as u64;

        // Toxicity score average across published reviews with scores
        let mut toxicity_sum = 0.0;
        let mut toxicity_count = 0usize;
        for rev in &published_reviews_vec {
            if let Some(score) = rev.toxicity_score {
                toxicity_sum += score;
                toxicity_count += 1;
            }
        }

        let avg_toxicity_score = if toxicity_count > 0 {
            let avg = toxicity_sum / (toxicity_count as f64);
            Some((avg * 10000.0).round() / 10000.0)
        } else {
            None
        };

        // Latest review timestamp across all reviews
        let latest_review_at = company_reviews.iter().map(|rev| rev.created_at).max();

        // Rating distribution: 1 to 5 stars initialized to 0
        let mut rating_distribution: HashMap<u8, u64> = (1..=5).map(|star| (star, 0)).collect();
        for rev in &published_reviews_vec {
            let star = rev.rating.clamp(1, 5);
            *rating_distribution.entry(star).or_insert(0) += 1;
        }

        // Top 3 topics by frequency
        let mut topic_counts: HashMap<String, u64> = HashMap::new();
        for rev in &published_reviews_vec {
            for topic in &rev.topics {
                if !topic.is_empty() {
                    *topic_counts.entry(topic.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut topics_sorted: Vec<(String, u64)> = topic_counts.into_iter().collect();
        // Sort descending by frequency, then ascending by topic name
        topics_sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let topics_top3 = topics_sorted
            .into_iter()
            .take(3)
            .map(|(topic, _)| topic)
            .collect();

        Ok(CompanyStats {
            total_reviews,
            published_reviews,
            avg_toxicity_score,
            latest_review_at,
            topics_top3,
            rating_distribution,
        })
    }

    /// Invalidate cache for a specific company slug
    pub fn invalidate_cache(&self, slug: &str) {
        if let Ok(norm) = canonical_slug(slug) {
            let mut cache = self.cache.write().unwrap();
            cache.remove(&norm);
        }
    }

    /// Invalidate all cached entries
    pub fn invalidate_all_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Telemetry cache metrics
    pub fn get_cache_metrics(&self) -> CacheMetrics {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_ratio = if total == 0 {
            0.0
        } else {
            let ratio = (hits as f64) / (total as f64);
            (ratio * 10000.0).round() / 10000.0
        };

        let cached_entries_count = self.cache.read().unwrap().len();

        CacheMetrics {
            hits,
            misses,
            hit_ratio,
            cached_entries_count,
        }
    }
}

/// Helper: canonicalize company slug
pub fn canonical_slug(slug: &str) -> Result<String, StatsError> {
    let trimmed = slug.trim();
    if trimmed.is_empty() {
        return Err(StatsError::InvalidSlug(
            "Company slug cannot be empty".to_string(),
        ));
    }
    if trimmed.len() > MAX_SLUG_LENGTH {
        return Err(StatsError::InvalidSlug(format!(
            "Company slug exceeds maximum length of {} characters",
            MAX_SLUG_LENGTH
        )));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(StatsError::InvalidSlug(
            "Company slug contains invalid control characters".to_string(),
        ));
    }
    Ok(trimmed.to_lowercase())
}
