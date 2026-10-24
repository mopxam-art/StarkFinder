use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReactionType {
    Upvote,
    Insightful,
    Thanks,
}

impl ReactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReactionType::Upvote => "upvote",
            ReactionType::Insightful => "insightful",
            ReactionType::Thanks => "thanks",
        }
    }
}

impl fmt::Display for ReactionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ReactionType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "upvote" => Ok(ReactionType::Upvote),
            "insightful" => Ok(ReactionType::Insightful),
            "thanks" => Ok(ReactionType::Thanks),
            _ => Err("invalid reaction type: must be upvote, insightful, or thanks"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReactionTarget {
    Review,
    Comment,
}

impl ReactionTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReactionTarget::Review => "review",
            ReactionTarget::Comment => "comment",
        }
    }
}

impl fmt::Display for ReactionTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ReactionTarget {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "review" | "reviews" => Ok(ReactionTarget::Review),
            "comment" | "comments" => Ok(ReactionTarget::Comment),
            _ => Err("invalid target type: must be review or comment"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
pub struct ReactionSummary {
    pub upvotes: u64,
    pub insightful: u64,
    pub thanks: u64,
    pub total: u64,
    pub user_reaction: Option<ReactionType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionRecord {
    pub id: String,
    pub user_id: String,
    pub target_type: ReactionTarget,
    pub target_id: String,
    pub reaction_type: ReactionType,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateReactionDto {
    pub reaction_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReactionResponse {
    pub target_type: String,
    pub target_id: String,
    pub status: String,
    pub summary: ReactionSummary,
}
