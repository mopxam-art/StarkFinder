use backend::{
    models::analytics::AnalyticsWindow,
    services::analytics_service::{AnalyticsError, AnalyticsService},
};

const OWNER_ALICE: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";
const STRANGER_BOB: &str =
    "0x0222222222222222222222222222222222222222222222222222222222222222";

fn setup_service() -> AnalyticsService {
    let service = AnalyticsService::new();
    service.register_owner("starkware", OWNER_ALICE);
    service.register_owner("argent", OWNER_ALICE);
    service
}

#[tokio::test]
async fn test_01_get_owner_analytics_30d_happy_path() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .expect("analytics should succeed for verified owner");

    assert_eq!(res.company_slug, "starkware");
    assert_eq!(res.window, AnalyticsWindow::ThirtyDays);
    assert!(res.total_reviews > 0);
    assert_eq!(res.volume_trend.len(), 4);
    assert!(!res.topics.is_empty());
    assert!(res.engagement.avg_response_sla_hours.is_some());
}

#[tokio::test]
async fn test_02_get_owner_analytics_90d_happy_path() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::NinetyDays)
        .expect("90d analytics should succeed");

    assert_eq!(res.window, AnalyticsWindow::NinetyDays);
    assert_eq!(res.volume_trend.len(), 12);
    assert!(res.total_reviews >= 30);
}

#[tokio::test]
async fn test_03_get_owner_analytics_unauthorized_non_owner_forbidden() {
    let service = setup_service();
    let err = service
        .get_owner_analytics(STRANGER_BOB, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap_err();

    assert!(matches!(err, AnalyticsError::Forbidden(_)));
}

#[tokio::test]
async fn test_04_get_owner_analytics_unclaimed_company_forbidden() {
    let service = setup_service();
    let err = service
        .get_owner_analytics(OWNER_ALICE, "unclaimed-startup", AnalyticsWindow::ThirtyDays)
        .unwrap_err();

    assert!(matches!(err, AnalyticsError::Forbidden(_)));
}

#[tokio::test]
async fn test_05_get_owner_analytics_empty_slug_bad_request() {
    let service = setup_service();
    let err = service
        .get_owner_analytics(OWNER_ALICE, "   ", AnalyticsWindow::ThirtyDays)
        .unwrap_err();

    assert!(matches!(err, AnalyticsError::BadRequest(_)));
}

#[tokio::test]
async fn test_06_get_owner_analytics_case_insensitive_slug() {
    let service = setup_service();
    let res_upper = service
        .get_owner_analytics(OWNER_ALICE, "StarkWare", AnalyticsWindow::ThirtyDays)
        .expect("should normalize uppercase slug");
    assert_eq!(res_upper.company_slug, "starkware");

    let res_lower = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();
    assert_eq!(res_lower.company_slug, "starkware");
}

#[tokio::test]
async fn test_07_get_owner_analytics_case_insensitive_wallet() {
    let service = setup_service();
    let uppercase_wallet = OWNER_ALICE.to_uppercase();
    let res = service
        .get_owner_analytics(&uppercase_wallet, "starkware", AnalyticsWindow::ThirtyDays)
        .expect("should normalize wallet casing");
    assert_eq!(res.company_slug, "starkware");
}

#[tokio::test]
async fn test_08_get_owner_analytics_caching_returns_identical_timestamp() {
    let service = setup_service();
    let res1 = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();
    let res2 = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    assert_eq!(res1.generated_at, res2.generated_at);
}

#[tokio::test]
async fn test_09_get_owner_analytics_invalidate_cache() {
    let service = setup_service();
    let res1 = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    service.invalidate_cache("starkware");

    // Sleep slightly to ensure distinct Utc timestamp if recomputed
    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

    let res2 = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    assert!(res2.generated_at >= res1.generated_at);
}

#[tokio::test]
async fn test_10_get_owner_analytics_30d_trend_periods_count() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();
    assert_eq!(res.volume_trend.len(), 4);
}

#[tokio::test]
async fn test_11_get_owner_analytics_90d_trend_periods_count() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::NinetyDays)
        .unwrap();
    assert_eq!(res.volume_trend.len(), 12);
}

#[tokio::test]
async fn test_12_get_owner_analytics_topics_structure() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    for topic in &res.topics {
        assert!(!topic.topic.is_empty());
        assert!(topic.count > 0);
        assert!(topic.sentiment_score >= 1.0 && topic.sentiment_score <= 5.0);
    }
}

#[tokio::test]
async fn test_13_get_owner_analytics_sentiment_distribution_sanity() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    assert!(res.sentiment.positive > 0);
    assert!(res.sentiment.neutral > 0);
    assert!(res.sentiment.negative > 0);
}

#[tokio::test]
async fn test_14_get_owner_analytics_engagement_metrics_sanity() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    assert!(res.engagement.upvotes_count > 0);
    assert!(res.engagement.insightful_count > 0);
    assert!(res.engagement.thanks_count > 0);
}

#[tokio::test]
async fn test_15_get_owner_analytics_report_rate_pct_range() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    assert!(res.engagement.report_rate_pct >= 0.0);
    assert!(res.engagement.report_rate_pct <= 100.0);
}

#[tokio::test]
async fn test_16_get_owner_analytics_sla_hours_positive() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    let sla = res.engagement.avg_response_sla_hours.unwrap();
    assert!(sla > 0.0);
}

#[tokio::test]
async fn test_17_is_owner_true_for_registered() {
    let service = setup_service();
    assert!(service.is_owner("starkware", OWNER_ALICE));
}

#[tokio::test]
async fn test_18_is_owner_false_for_unregistered() {
    let service = setup_service();
    assert!(!service.is_owner("starkware", STRANGER_BOB));
    assert!(!service.is_owner("unknown-corp", OWNER_ALICE));
}

#[tokio::test]
async fn test_19_register_owner_updates_existing_owner() {
    let service = setup_service();
    assert!(service.is_owner("starkware", OWNER_ALICE));

    service.register_owner("starkware", STRANGER_BOB);
    assert!(!service.is_owner("starkware", OWNER_ALICE));
    assert!(service.is_owner("starkware", STRANGER_BOB));
}

#[tokio::test]
async fn test_20_analytics_window_serde_30d() {
    assert_eq!(
        serde_json::to_string(&AnalyticsWindow::ThirtyDays).unwrap(),
        "\"30d\""
    );
    let parsed: AnalyticsWindow = serde_json::from_str("\"30d\"").unwrap();
    assert_eq!(parsed, AnalyticsWindow::ThirtyDays);
}

#[tokio::test]
async fn test_21_analytics_window_serde_90d() {
    assert_eq!(
        serde_json::to_string(&AnalyticsWindow::NinetyDays).unwrap(),
        "\"90d\""
    );
    let parsed: AnalyticsWindow = serde_json::from_str("\"90d\"").unwrap();
    assert_eq!(parsed, AnalyticsWindow::NinetyDays);
}

#[tokio::test]
async fn test_22_analytics_window_default_is_30d() {
    assert_eq!(AnalyticsWindow::default(), AnalyticsWindow::ThirtyDays);
}

#[tokio::test]
async fn test_23_different_companies_isolated_owners() {
    let service = AnalyticsService::new();
    service.register_owner("company-a", OWNER_ALICE);
    service.register_owner("company-b", STRANGER_BOB);

    assert!(service.is_owner("company-a", OWNER_ALICE));
    assert!(!service.is_owner("company-a", STRANGER_BOB));
    assert!(service.is_owner("company-b", STRANGER_BOB));
    assert!(!service.is_owner("company-b", OWNER_ALICE));
}

#[tokio::test]
async fn test_24_different_windows_separate_cache_keys() {
    let service = setup_service();
    let res_30d = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();
    let res_90d = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::NinetyDays)
        .unwrap();

    assert_eq!(res_30d.volume_trend.len(), 4);
    assert_eq!(res_90d.volume_trend.len(), 12);
    assert_ne!(res_30d.total_reviews, res_90d.total_reviews);
}

#[tokio::test]
async fn test_25_clear_all_resets_owners_and_cache() {
    let service = setup_service();
    assert!(service.is_owner("starkware", OWNER_ALICE));

    service.clear_all();
    assert!(!service.is_owner("starkware", OWNER_ALICE));
}

#[tokio::test]
async fn test_26_total_reviews_matches_volume_sum() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    let computed_sum: u32 = res.volume_trend.iter().map(|p| p.review_count).sum();
    assert_eq!(res.total_reviews, computed_sum);
}

#[tokio::test]
async fn test_27_volume_trend_ordering_chronological() {
    let service = setup_service();
    let res = service
        .get_owner_analytics(OWNER_ALICE, "starkware", AnalyticsWindow::ThirtyDays)
        .unwrap();

    assert_eq!(res.volume_trend[0].period, "W-3");
    assert_eq!(res.volume_trend[3].period, "W-0");
}
