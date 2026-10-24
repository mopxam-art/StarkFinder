use axum::http::StatusCode;
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::models::search::{SearchItemType, SearchResponse};
use backend::services::search_service::{SEARCH_SERVICE, generate_snippet};

fn setup_test_app() -> TestServer {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

// =========================================================================
// SECTION 1: UNIT TESTS - SNIPPET GENERATION & SCORING
// =========================================================================

#[test]
fn test_generate_snippet_contains_mark_tag() {
    let text = "StarkWare develops STARK-based scalability solutions for Ethereum.";
    let snippet = generate_snippet(text, "STARK");
    assert!(snippet.contains("<mark>STARK</mark>"));
}

#[test]
fn test_generate_snippet_preserves_case_in_highlight() {
    let text = "The Argent smart wallet provides account abstraction.";
    let snippet = generate_snippet(text, "argent");
    assert!(snippet.contains("<mark>Argent</mark>"));
}

#[test]
fn test_generate_snippet_adds_ellipsis_for_long_text() {
    let long_text = format!(
        "{} TargetKeyword {}",
        "A".repeat(150),
        "B".repeat(150)
    );
    let snippet = generate_snippet(&long_text, "TargetKeyword");
    assert!(snippet.starts_with("... "));
    assert!(snippet.ends_with(" ..."));
    assert!(snippet.contains("<mark>TargetKeyword</mark>"));
}

#[test]
fn test_generate_snippet_empty_text() {
    let snippet = generate_snippet("", "query");
    assert_eq!(snippet, "");
}

// =========================================================================
// SECTION 2: INTEGRATION TESTS - HTTP GET /search
// =========================================================================

#[tokio::test]
async fn test_search_rejects_empty_query() {
    let app = setup_test_app();
    let res = app.get("/search?q=").await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = res.json();
    assert_eq!(json["error"], "query must be at least 2 characters");
}

#[tokio::test]
async fn test_search_rejects_single_character_query() {
    let app = setup_test_app();
    let res = app.get("/search?q=a").await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = res.json();
    assert_eq!(json["error"], "query must be at least 2 characters");
}

#[tokio::test]
async fn test_search_rejects_whitespace_query() {
    let app = setup_test_app();
    let res = app.get("/search?q=%20%20%20").await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = res.json();
    assert_eq!(json["error"], "query must be at least 2 characters");
}

#[tokio::test]
async fn test_search_missing_q_parameter() {
    let app = setup_test_app();
    let res = app.get("/search").await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = res.json();
    assert_eq!(json["error"], "query must be at least 2 characters");
}

#[tokio::test]
async fn test_search_scope_companies_only() {
    let app = setup_test_app();
    let res = app.get("/search?q=wallet&scope=companies").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert_eq!(resp.scope, "companies");
    assert!(!resp.items.is_empty());
    for item in &resp.items {
        assert_eq!(item.item_type, SearchItemType::Company);
    }
}

#[tokio::test]
async fn test_search_scope_reviews_only() {
    let app = setup_test_app();
    let res = app.get("/search?q=wallet&scope=reviews").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert_eq!(resp.scope, "reviews");
    assert!(!resp.items.is_empty());
    for item in &resp.items {
        assert_eq!(item.item_type, SearchItemType::Review);
    }
}

#[tokio::test]
async fn test_search_scope_all_returns_both() {
    let app = setup_test_app();
    let res = app.get("/search?q=wallet&scope=all").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert_eq!(resp.scope, "all");
    assert!(!resp.items.is_empty());

    let has_company = resp.items.iter().any(|i| i.item_type == SearchItemType::Company);
    let has_review = resp.items.iter().any(|i| i.item_type == SearchItemType::Review);
    assert!(has_company);
    assert!(has_review);
}

#[tokio::test]
async fn test_search_default_scope_is_all() {
    let app = setup_test_app();
    let res = app.get("/search?q=wallet").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert_eq!(resp.scope, "all");
}

#[tokio::test]
async fn test_search_exact_match_ranks_highest() {
    let app = setup_test_app();
    let res = app.get("/search?q=Argent").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert!(!resp.items.is_empty());

    // The company "argent" should be ranked #1 due to exact match
    let first = &resp.items[0];
    assert_eq!(first.item_type, SearchItemType::Company);
    assert_eq!(first.id_or_slug, "argent");
    assert!(first.score >= 10.0);
}

#[tokio::test]
async fn test_search_case_insensitivity() {
    let app = setup_test_app();
    let res_upper = app.get("/search?q=STARKWARE").await;
    let res_lower = app.get("/search?q=starkware").await;
    assert_eq!(res_upper.status_code(), StatusCode::OK);
    assert_eq!(res_lower.status_code(), StatusCode::OK);

    let upper: SearchResponse = res_upper.json();
    let lower: SearchResponse = res_lower.json();

    assert_eq!(upper.total, lower.total);
    assert_eq!(upper.items.first().map(|i| &i.id_or_slug), lower.items.first().map(|i| &i.id_or_slug));
}

#[tokio::test]
async fn test_search_snippet_contains_highlighting() {
    let app = setup_test_app();
    let res = app.get("/search?q=liquidity").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert!(!resp.items.is_empty());

    let match_with_highlight = resp
        .items
        .iter()
        .any(|i| i.snippet.to_lowercase().contains("<mark>liquidity</mark>"));
    assert!(match_with_highlight);
}

#[tokio::test]
async fn test_search_limit_clamped_to_max_25() {
    let app = setup_test_app();

    // Index 30 documents
    for i in 0..30 {
        SEARCH_SERVICE.index_company(
            &format!("batch-company-{:02}", i),
            &format!("Batch Company {:02}", i),
            "Batch indexing for limit clamp testing on Starknet ecosystem.",
            &["batch", "testing"],
        );
    }

    let res = app.get("/search?q=Batch&limit=100").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert!(resp.items.len() <= 25);
    assert_eq!(resp.items.len(), 25);
    assert!(resp.has_more);
}

#[tokio::test]
async fn test_search_limit_custom_pagination() {
    let app = setup_test_app();
    let res = app.get("/search?q=wallet&limit=2").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert_eq!(resp.items.len(), 2);
    assert!(resp.has_more);
}

#[tokio::test]
async fn test_search_pagination_offset() {
    let app = setup_test_app();
    let res_page1 = app.get("/search?q=wallet&limit=2&offset=0").await;
    let res_page2 = app.get("/search?q=wallet&limit=2&offset=2").await;

    let p1: SearchResponse = res_page1.json();
    let p2: SearchResponse = res_page2.json();

    assert_eq!(p1.items.len(), 2);
    assert!(!p2.items.is_empty());
    assert_ne!(p1.items[0].id_or_slug, p2.items[0].id_or_slug);
    assert_ne!(p1.items[1].id_or_slug, p2.items[0].id_or_slug);
}

#[tokio::test]
async fn test_search_offset_beyond_total_returns_empty() {
    let app = setup_test_app();
    let res = app.get("/search?q=wallet&offset=9999").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert!(resp.items.is_empty());
    assert!(!resp.has_more);
}

#[tokio::test]
async fn test_search_no_results() {
    let app = setup_test_app();
    let res = app.get("/search?q=zzzznonexistenttoken999").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert_eq!(resp.total, 0);
    assert!(resp.items.is_empty());
    assert!(!resp.has_more);
}

#[tokio::test]
async fn test_search_multi_word_query() {
    let app = setup_test_app();
    let res = app.get("/search?q=smart%20contract%20wallet").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert!(!resp.items.is_empty());
    // Argent or Braavos should score very high
    let top_slugs: Vec<&str> = resp.items.iter().take(2).map(|i| i.id_or_slug.as_str()).collect();
    assert!(top_slugs.contains(&"argent") || top_slugs.contains(&"braavos"));
}

#[tokio::test]
async fn test_search_tag_matching() {
    let app = setup_test_app();
    let res = app.get("/search?q=zk-rollups").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert!(!resp.items.is_empty());
    assert_eq!(resp.items[0].id_or_slug, "starkware");
}

#[tokio::test]
async fn test_search_dynamic_document_indexing() {
    let app = setup_test_app();

    let unique_term = "QuantumXPro";
    let res_before = app.get(&format!("/search?q={}", unique_term)).await;
    let before: SearchResponse = res_before.json();
    assert_eq!(before.total, 0);

    SEARCH_SERVICE.index_company(
        "quantum-labs",
        "Quantum Labs",
        "Building QuantumXPro next-gen cryptography on Starknet.",
        &["quantum", "crypto"],
    );

    let res_after = app.get(&format!("/search?q={}", unique_term)).await;
    let after: SearchResponse = res_after.json();
    assert_eq!(after.total, 1);
    assert_eq!(after.items[0].id_or_slug, "quantum-labs");
    assert!(after.items[0].snippet.contains("<mark>QuantumXPro</mark>"));
}

#[tokio::test]
async fn test_search_trigram_fuzzy_match() {
    let app = setup_test_app();
    // Typo: "starkwr" instead of "starkware"
    let res = app.get("/search?q=starkwr").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: SearchResponse = res.json();
    assert!(!resp.items.is_empty());
    assert!(resp.items.iter().any(|i| i.id_or_slug == "starkware"));
}
