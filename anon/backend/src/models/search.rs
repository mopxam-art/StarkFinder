use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Type of entity returned in search results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchItemType {
    #[serde(alias = "Review", alias = "review")]
    Review,
    #[serde(alias = "Company", alias = "company")]
    Company,
}

/// An individual matched item in the search results
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SearchResultItem {
    pub item_type: SearchItemType,
    pub id_or_slug: String,
    pub title: Option<String>,
    pub score: f64,
    pub snippet: String,
}

/// Response payload containing matched items, pagination, and query metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    pub query: String,
    pub scope: String,
    pub total: usize,
    pub items: Vec<SearchResultItem>,
    pub has_more: bool,
}

/// Query parameters for the /search endpoint
#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct SearchQuery {
    /// Search term (minimum 2 non-whitespace characters)
    pub q: Option<String>,
    /// Search scope: 'reviews', 'companies', or 'all' (default: 'all')
    pub scope: Option<String>,
    /// Number of items to return (default: 10, max: 25)
    pub limit: Option<usize>,
    /// Pagination offset (default: 0)
    pub offset: Option<usize>,
}
