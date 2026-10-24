use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Overall interview impression rating
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum OverallExperience {
    #[default]
    Positive,
    Neutral,
    Negative,
}

/// A specific interview question asked during the interview process
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InterviewQuestion {
    pub question: String,
    pub category: Option<String>,
}

/// A structured interview review record submitted by a candidate
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InterviewReview {
    pub id: Uuid,
    pub company_slug: String,
    pub author_wallet: String,
    pub role_title: String,
    pub location: Option<String>,
    pub experience_level: Option<String>,
    pub overall_experience: OverallExperience,
    pub difficulty: u8,
    pub process_summary: String,
    pub questions: Vec<InterviewQuestion>,
    pub timeframe: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Request payload submitted by a user to create an interview review
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema, Default)]
pub struct CreateInterviewRequest {
    pub company_slug: String,
    pub role_title: String,
    pub location: Option<String>,
    pub experience_level: Option<String>,
    pub overall_experience: OverallExperience,
    pub difficulty: u8,
    pub process_summary: String,
    pub questions: Vec<InterviewQuestion>,
    pub timeframe: Option<String>,
}

/// Paginated list of interview reviews returned for a company
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InterviewListResponse {
    pub items: Vec<InterviewReview>,
    pub total: usize,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
