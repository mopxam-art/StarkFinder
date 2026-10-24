use axum::{
    Json,
    extract::Path,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::IntoResponse,
};
use serde_json::json;

use crate::{
    models::company_stats::{CacheMetrics, CompanyStats},
    services::stats_service::{STATS_SERVICE, StatsError},
};

/// Fetch aggregated review statistics for a company with lightweight in-memory caching
#[utoipa::path(
    get,
    path = "/companies/{slug}/stats",
    tag = "stats",
    params(
        ("slug" = String, Path, description = "Company identifier slug")
    ),
    responses(
        (status = 200, description = "Company aggregate statistics", body = CompanyStats,
         headers(
             ("X-Cache-Status" = String, description = "Cache status: HIT or MISS")
         )
        ),
        (status = 400, description = "Invalid slug parameter", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Company not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_company_stats(
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, StatsError> {
    let (stats, cache_status) = STATS_SERVICE.get_company_stats_with_status(&slug)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-cache-status"),
        HeaderValue::from_static(cache_status.as_header_str()),
    );

    Ok((StatusCode::OK, headers, Json(stats)))
}

/// Retrieve telemetry metrics for company stats cache
#[utoipa::path(
    get,
    path = "/metrics/stats-cache",
    tag = "stats",
    responses(
        (status = 200, description = "Cache telemetry metrics", body = CacheMetrics)
    )
)]
pub async fn get_stats_cache_metrics() -> Json<CacheMetrics> {
    Json(STATS_SERVICE.get_cache_metrics())
}

/// Invalidate cache for a specific company slug
#[utoipa::path(
    post,
    path = "/companies/{slug}/invalidate-cache",
    tag = "stats",
    params(
        ("slug" = String, Path, description = "Company identifier slug")
    ),
    responses(
        (status = 200, description = "Cache invalidated successfully")
    )
)]
pub async fn invalidate_company_cache(
    Path(slug): Path<String>,
) -> impl IntoResponse {
    STATS_SERVICE.invalidate_cache(&slug);
    (
        StatusCode::OK,
        Json(json!({
            "message": "Cache invalidated successfully",
            "slug": slug
        })),
    )
}
