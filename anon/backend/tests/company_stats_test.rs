use std::time::Duration;

use axum::http::StatusCode;
use axum_test::TestServer;
use chrono::{TimeZone, Utc};

use backend::libs::db::AppState;
use backend::models::company_stats::{CacheMetrics, CompanyStats, ReviewStatus};
use backend::services::stats_service::STATS_SERVICE;

/// Spin up an ephemeral test server instance
fn setup_test_app() -> TestServer {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

// ============================================================================
// GROUP 1: AGGREGATE CALCULATIONS & METRICS (Tests 01-07)
// ============================================================================

#[tokio::test]
async fn test_01_empty_company_stats() {
    let server = setup_test_app();
    let slug = "empty-corp-01";
    STATS_SERVICE.register_company(slug).unwrap();

    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res.status_code(), StatusCode::OK);

    let stats: CompanyStats = res.json();
    assert_eq!(stats.total_reviews, 0);
    assert_eq!(stats.published_reviews, 0);
    assert_eq!(stats.avg_toxicity_score, None);
    assert_eq!(stats.latest_review_at, None);
    assert!(stats.topics_top3.is_empty());
    assert_eq!(stats.rating_distribution.get(&1), Some(&0));
    assert_eq!(stats.rating_distribution.get(&5), Some(&0));
}

#[tokio::test]
async fn test_02_total_vs_published_filtering() {
    let server = setup_test_app();
    let slug = "filter-corp-02";

    // 3 published, 2 pending, 1 archived, 1 rejected
    for _ in 0..3 {
        STATS_SERVICE
            .add_review(slug, 5, ReviewStatus::Published, None, vec![], None)
            .unwrap();
    }
    for _ in 0..2 {
        STATS_SERVICE
            .add_review(slug, 4, ReviewStatus::Pending, None, vec![], None)
            .unwrap();
    }
    STATS_SERVICE
        .add_review(slug, 2, ReviewStatus::Archived, None, vec![], None)
        .unwrap();
    STATS_SERVICE
        .add_review(slug, 1, ReviewStatus::Rejected, None, vec![], None)
        .unwrap();

    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let stats: CompanyStats = res.json();

    assert_eq!(stats.total_reviews, 7);
    assert_eq!(stats.published_reviews, 3);
}

#[tokio::test]
async fn test_03_avg_toxicity_score_calculation() {
    let server = setup_test_app();
    let slug = "toxic-calc-03";

    // Scores: 0.1, 0.4, 0.7 -> avg = 0.4
    STATS_SERVICE
        .add_review(slug, 4, ReviewStatus::Published, Some(0.1), vec![], None)
        .unwrap();
    STATS_SERVICE
        .add_review(slug, 5, ReviewStatus::Published, Some(0.4), vec![], None)
        .unwrap();
    STATS_SERVICE
        .add_review(slug, 3, ReviewStatus::Published, Some(0.7), vec![], None)
        .unwrap();

    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let stats: CompanyStats = res.json();

    assert_eq!(stats.avg_toxicity_score, Some(0.4));
}

#[tokio::test]
async fn test_04_avg_toxicity_ignores_non_published_and_none() {
    let server = setup_test_app();
    let slug = "toxic-filter-04";

    // Published with 0.2
    STATS_SERVICE
        .add_review(slug, 4, ReviewStatus::Published, Some(0.2), vec![], None)
        .unwrap();
    // Published without score
    STATS_SERVICE
        .add_review(slug, 5, ReviewStatus::Published, None, vec![], None)
        .unwrap();
    // Pending review with 1.0 (must NOT be counted)
    STATS_SERVICE
        .add_review(slug, 1, ReviewStatus::Pending, Some(1.0), vec![], None)
        .unwrap();

    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let stats: CompanyStats = res.json();

    assert_eq!(stats.avg_toxicity_score, Some(0.2));
}

#[tokio::test]
async fn test_05_rating_distribution_calculation() {
    let server = setup_test_app();
    let slug = "ratings-dist-05";

    // Ratings: 5, 5, 5, 4, 3, 1
    for &star in &[5, 5, 5, 4, 3, 1] {
        STATS_SERVICE
            .add_review(slug, star, ReviewStatus::Published, None, vec![], None)
            .unwrap();
    }

    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let stats: CompanyStats = res.json();

    assert_eq!(stats.rating_distribution.get(&5), Some(&3));
    assert_eq!(stats.rating_distribution.get(&4), Some(&1));
    assert_eq!(stats.rating_distribution.get(&3), Some(&1));
    assert_eq!(stats.rating_distribution.get(&2), Some(&0));
    assert_eq!(stats.rating_distribution.get(&1), Some(&1));
}

#[tokio::test]
async fn test_06_topics_top3_frequency_and_sorting() {
    let server = setup_test_app();
    let slug = "topics-top3-06";

    // Topic frequencies:
    // "defi": 4 times
    // "starknet": 3 times
    // "security": 2 times
    // "wallet": 1 time
    STATS_SERVICE
        .add_review(
            slug,
            5,
            ReviewStatus::Published,
            None,
            vec!["defi".into(), "starknet".into()],
            None,
        )
        .unwrap();
    STATS_SERVICE
        .add_review(
            slug,
            4,
            ReviewStatus::Published,
            None,
            vec!["defi".into(), "security".into()],
            None,
        )
        .unwrap();
    STATS_SERVICE
        .add_review(
            slug,
            5,
            ReviewStatus::Published,
            None,
            vec!["defi".into(), "starknet".into()],
            None,
        )
        .unwrap();
    STATS_SERVICE
        .add_review(
            slug,
            5,
            ReviewStatus::Published,
            None,
            vec!["defi".into(), "starknet".into(), "security".into(), "wallet".into()],
            None,
        )
        .unwrap();

    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let stats: CompanyStats = res.json();

    assert_eq!(stats.topics_top3, vec!["defi", "starknet", "security"]);
}

#[tokio::test]
async fn test_07_latest_review_at_tracks_most_recent() {
    let server = setup_test_app();
    let slug = "latest-ts-07";

    let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2026, 6, 15, 10, 30, 0).unwrap();
    let t3 = Utc.with_ymd_and_hms(2026, 10, 24, 18, 0, 0).unwrap();

    STATS_SERVICE
        .add_review(slug, 4, ReviewStatus::Published, None, vec![], Some(t1))
        .unwrap();
    STATS_SERVICE
        .add_review(slug, 5, ReviewStatus::Published, None, vec![], Some(t3))
        .unwrap();
    STATS_SERVICE
        .add_review(slug, 3, ReviewStatus::Published, None, vec![], Some(t2))
        .unwrap();

    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let stats: CompanyStats = res.json();

    assert_eq!(stats.latest_review_at, Some(t3));
}

// ============================================================================
// GROUP 2: CACHING LAYER & TELEMETRY (Tests 08-14)
// ============================================================================

#[tokio::test]
async fn test_08_initial_request_cache_miss() {
    let server = setup_test_app();
    let slug = "cache-miss-08";
    STATS_SERVICE.register_company(slug).unwrap();

    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res.status_code(), StatusCode::OK);

    let cache_header = res.header("x-cache-status");
    assert_eq!(cache_header, "MISS");
}

#[tokio::test]
async fn test_09_subsequent_request_cache_hit() {
    let server = setup_test_app();
    let slug = "cache-hit-09";
    STATS_SERVICE.register_company(slug).unwrap();

    // 1st request -> MISS
    let res1 = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res1.header("x-cache-status"), "MISS");

    // 2nd request -> HIT
    let res2 = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res2.header("x-cache-status"), "HIT");

    // Data should match exactly
    let body1: CompanyStats = res1.json();
    let body2: CompanyStats = res2.json();
    assert_eq!(body1, body2);
}

#[tokio::test]
async fn test_10_cache_metrics_telemetry() {
    let server = setup_test_app();
    let slug = "metrics-corp-10";
    STATS_SERVICE.register_company(slug).unwrap();

    // Trigger 1 miss and 3 hits
    server.get(&format!("/companies/{}/stats", slug)).await; // MISS
    server.get(&format!("/companies/{}/stats", slug)).await; // HIT
    server.get(&format!("/companies/{}/stats", slug)).await; // HIT
    server.get(&format!("/companies/{}/stats", slug)).await; // HIT

    let res = server.get("/metrics/stats-cache").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let metrics: CacheMetrics = res.json();

    assert!(metrics.hits >= 3);
    assert!(metrics.misses >= 1);
    assert!(metrics.hit_ratio > 0.0 && metrics.hit_ratio <= 1.0);
    assert!(metrics.cached_entries_count >= 1);
}

#[tokio::test]
async fn test_11_cache_invalidation_via_new_review() {
    let server = setup_test_app();
    let slug = "inval-review-11";
    STATS_SERVICE.register_company(slug).unwrap();

    // Warm up cache -> HIT
    server.get(&format!("/companies/{}/stats", slug)).await; // MISS
    let res_hit = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res_hit.header("x-cache-status"), "HIT");

    // Adding review must invalidate cache
    STATS_SERVICE
        .add_review(slug, 5, ReviewStatus::Published, None, vec![], None)
        .unwrap();

    // Subsequent call must be MISS because cache was cleared
    let res_after = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res_after.header("x-cache-status"), "MISS");
    let stats: CompanyStats = res_after.json();
    assert_eq!(stats.published_reviews, 1);
}

#[tokio::test]
async fn test_12_cache_invalidation_via_endpoint() {
    let server = setup_test_app();
    let slug = "inval-endpoint-12";
    STATS_SERVICE.register_company(slug).unwrap();

    // Populate cache
    server.get(&format!("/companies/{}/stats", slug)).await;
    let hit = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(hit.header("x-cache-status"), "HIT");

    // Invalidate via POST endpoint
    let inv_res = server
        .post(&format!("/companies/{}/invalidate-cache", slug))
        .await;
    assert_eq!(inv_res.status_code(), StatusCode::OK);

    // Next request must be MISS
    let after_inv = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(after_inv.header("x-cache-status"), "MISS");
}

#[tokio::test]
async fn test_13_cache_ttl_expiration() {
    let server = setup_test_app();
    let slug = "ttl-expire-13";
    STATS_SERVICE.register_company(slug).unwrap();

    // Set 1-second TTL
    STATS_SERVICE.set_custom_ttl(slug, 1).unwrap();

    // First request -> MISS
    let res1 = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res1.header("x-cache-status"), "MISS");

    // Immediate request -> HIT
    let res2 = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res2.header("x-cache-status"), "HIT");

    // Wait 1.1s for TTL expiration
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // After expiration -> MISS (recalculation)
    let res3 = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res3.header("x-cache-status"), "MISS");
}

#[tokio::test]
async fn test_14_cache_hit_latency_benchmark() {
    let server = setup_test_app();
    let slug = "latency-bench-14";
    STATS_SERVICE.register_company(slug).unwrap();

    // Warm up
    server.get(&format!("/companies/{}/stats", slug)).await;

    let start = std::time::Instant::now();
    let res = server.get(&format!("/companies/{}/stats", slug)).await;
    let elapsed = start.elapsed();

    assert_eq!(res.header("x-cache-status"), "HIT");
    // In-memory hit must resolve in < 25ms even under test harness overhead
    assert!(
        elapsed < Duration::from_millis(25),
        "Cache hit took too long: {:?}",
        elapsed
    );
}

// ============================================================================
// GROUP 3: EDGE CASES & CONTRACTS (Tests 15-18)
// ============================================================================

#[tokio::test]
async fn test_15_nonexistent_company_returns_404() {
    let server = setup_test_app();
    let res = server.get("/companies/nonexistent-org-15/stats").await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_16_empty_or_whitespace_slug_returns_400() {
    let server = setup_test_app();
    let res = server.get("/companies/%20%20/stats").await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_17_slug_case_insensitivity() {
    let server = setup_test_app();
    let slug = "argent-17";
    STATS_SERVICE.register_company(slug).unwrap();

    // Lowercase miss
    let res1 = server.get(&format!("/companies/{}/stats", slug)).await;
    assert_eq!(res1.header("x-cache-status"), "MISS");

    // Uppercase request maps to same cache entry -> HIT
    let res2 = server.get("/companies/ARGENT-17/stats").await;
    assert_eq!(res2.status_code(), StatusCode::OK);
    assert_eq!(res2.header("x-cache-status"), "HIT");
}

#[tokio::test]
async fn test_18_multiple_companies_cache_isolation() {
    let server = setup_test_app();
    let corp_a = "corp-a-18";
    let corp_b = "corp-b-18";

    STATS_SERVICE.register_company(corp_a).unwrap();
    STATS_SERVICE.register_company(corp_b).unwrap();

    // Request Corp A -> MISS
    let res_a1 = server.get(&format!("/companies/{}/stats", corp_a)).await;
    assert_eq!(res_a1.header("x-cache-status"), "MISS");

    // Request Corp B -> MISS (must not hit Corp A cache)
    let res_b1 = server.get(&format!("/companies/{}/stats", corp_b)).await;
    assert_eq!(res_b1.header("x-cache-status"), "MISS");

    // Subsequent A -> HIT
    let res_a2 = server.get(&format!("/companies/{}/stats", corp_a)).await;
    assert_eq!(res_a2.header("x-cache-status"), "HIT");
}
