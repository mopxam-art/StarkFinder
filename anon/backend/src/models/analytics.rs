use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Analytics time window selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
pub enum AnalyticsWindow {
    #[default]
    #[serde(rename = "30d")]
    ThirtyDays,
    #[serde(rename = "90d")]
    NinetyDays,
}

/// Periodic volume aggregate data point
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VolumeTrendPoint {
    pub period: String,
    pub review_count: u32,
    pub interview_count: u32,
}

/// Prevalence and sentiment metrics for a specific topic/tag
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TopicPrevalence {
    pub topic: String,
    pub count: u32,
    pub sentiment_score: f32,
}

/// Sentiment distribution breakdown
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
pub struct SentimentDistribution {
    pub positive: u32,
    pub neutral: u32,
    pub negative: u32,
}

/// Engagement and response metrics for company reviews
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, Default)]
pub struct EngagementMetrics {
    pub upvotes_count: u32,
    pub insightful_count: u32,
    pub thanks_count: u32,
    pub report_rate_pct: f32,
    pub avg_response_sla_hours: Option<f32>,
}

/// Comprehensive deep-dive analytics response for claimed company owners
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OwnerAnalyticsResponse {
    pub company_slug: String,
    pub window: AnalyticsWindow,
    pub total_reviews: u32,
    pub volume_trend: Vec<VolumeTrendPoint>,
    pub topics: Vec<TopicPrevalence>,
    pub sentiment: SentimentDistribution,
    pub engagement: EngagementMetrics,
    pub generated_at: DateTime<Utc>,
}
