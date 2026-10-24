use axum::{
    Json,
    extract::Query,
};

use crate::{
    models::search::{SearchQuery, SearchResponse},
    services::search_service::{SEARCH_SERVICE, SearchError},
};

/// Search reviews and companies with keyword ranking and snippet highlighting
#[utoipa::path(
    get,
    path = "/search",
    tag = "search",
    params(
        SearchQuery
    ),
    responses(
        (status = 200, description = "Search query executed successfully", body = SearchResponse),
        (status = 400, description = "Query must be at least 2 characters", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn search(
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, SearchError> {
    let q = params.q.unwrap_or_default();
    let resp = SEARCH_SERVICE.search(
        &q,
        params.scope.as_deref(),
        params.limit,
        params.offset,
    )?;

    Ok(Json(resp))
}
