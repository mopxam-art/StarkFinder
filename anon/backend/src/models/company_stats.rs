use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Review publication lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Published,
    Pending,
    Archived,
    Rejected,
}

/// Internal review record for aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyReview {
    pub id: String,
    pub company_slug: String,
    pub rating: u8, // 1 to 5 stars
    pub status: ReviewStatus,
    pub toxicity_score: Option<f64>,
    pub topics: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Company statistics and review aggregates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CompanyStats {
    pub total_reviews: u64,
    pub published_reviews: u64,
    pub avg_toxicity_score: Option<f64>,
    pub latest_review_at: Option<DateTime<Utc>>,
    pub topics_top3: Vec<String>,
    pub rating_distribution: HashMap<u8, u64>,
}

/// Real-time cache telemetry metrics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub hit_ratio: f64,
    pub cached_entries_count: usize,
}

/// DTO for creating a sample company review
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateReviewDto {
    pub rating: u8,
    #[serde(default = "default_published")]
    pub status: ReviewStatus,
    pub toxicity_score: Option<f64>,
    #[serde(default)]
    pub topics: Vec<String>,
}

fn default_published() -> ReviewStatus {
    ReviewStatus::Published
}
