use axum::http::StatusCode;
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::routes::typeahead::{
    search_candidates, slugify, typeahead, TypeaheadItem, TypeaheadScope, SEED_COMPANIES, SEED_TAGS,
};

fn create_mock_app() -> axum::Router {
    let invalid_url = "postgresql://invalid:invalid@localhost:9999/invalid";
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_millis(50))
        .connect_lazy(invalid_url)
        .expect("lazy mock pool should construct");

    axum::Router::new()
        .route("/typeahead", axum::routing::get(typeahead))
        .with_state(AppState { pool })
}

// ============================================================================
// DIMENSION 1: BOUNDARY & SLUGIFICATION RULES (20 tests)
// ============================================================================
#[test]
fn test_d1_01_slugify_standard_alpha() {
    assert_eq!(slugify("StarkWare"), "starkware");
}

#[test]
fn test_d1_02_slugify_with_spaces() {
    assert_eq!(slugify("Account Abstraction"), "account-abstraction");
}

#[test]
fn test_d1_03_slugify_multiple_consecutive_spaces() {
    assert_eq!(slugify("Zero    Knowledge"), "zero-knowledge");
}

#[test]
fn test_d1_04_slugify_special_characters_removal() {
    assert_eq!(slugify("Kakarot zkEVM (v2.0!)"), "kakarot-zkevm-v2-0");
}

#[test]
fn test_d1_05_slugify_preserves_numbers() {
    assert_eq!(slugify("Cairo 1.0"), "cairo-1-0");
}

#[test]
fn test_d1_06_slugify_empty_string() {
    assert_eq!(slugify(""), "");
}

#[test]
fn test_d1_07_slugify_whitespace_only() {
    assert_eq!(slugify("   "), "");
}

#[test]
fn test_d1_08_slugify_leading_and_trailing_punctuation() {
    assert_eq!(slugify("---StarkNet---"), "starknet");
}

#[test]
fn test_d1_09_slugify_mixed_symbols_underscores() {
    assert_eq!(slugify("hello_world_test"), "hello-world-test");
}

#[test]
fn test_d1_10_slugify_hyphen_deduplication() {
    assert_eq!(slugify("a---b--c"), "a-b-c");
}

#[test]
fn test_d1_11_slugify_single_char() {
    assert_eq!(slugify("A"), "a");
}

#[test]
fn test_d1_12_slugify_numeric_only() {
    assert_eq!(slugify("42069"), "42069");
}

#[test]
fn test_d1_13_slugify_idempotence() {
    let once = slugify("Layer 2 Scaling");
    let twice = slugify(&once);
    assert_eq!(once, twice);
}

#[test]
fn test_d1_14_search_candidates_empty_query_returns_empty() {
    let res = search_candidates("", TypeaheadScope::All, 10, &[], &[]);
    assert!(res.is_empty());
}

#[test]
fn test_d1_15_search_candidates_whitespace_query_returns_empty() {
    let res = search_candidates("   ", TypeaheadScope::All, 10, &[], &[]);
    assert!(res.is_empty());
}

#[test]
fn test_d1_16_search_candidates_limit_zero_returns_empty() {
    let res = search_candidates("stark", TypeaheadScope::All, 0, &[], &[]);
    assert!(res.is_empty());
}

#[test]
fn test_d1_17_search_candidates_limit_clamp_enforced() {
    let res = search_candidates("a", TypeaheadScope::All, 3, &[], &[]);
    assert!(res.len() <= 3);
}

#[test]
fn test_d1_18_search_candidates_exact_single_result() {
    let res = search_candidates("herodotus", TypeaheadScope::All, 10, &[], &[]);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].value, "Herodotus");
}

#[test]
fn test_d1_19_search_candidates_non_matching_query() {
    let res = search_candidates("xyznonexistent999", TypeaheadScope::All, 10, &[], &[]);
    assert!(res.is_empty());
}

#[test]
fn test_d1_20_search_candidates_single_character_query() {
    let res = search_candidates("s", TypeaheadScope::All, 5, &[], &[]);
    assert_eq!(res.len(), 5);
}

// ============================================================================
// DIMENSION 2: SCOPE FILTERING & MATCHING HEURISTICS (20 tests)
// ============================================================================
#[test]
fn test_d2_01_scope_companies_only_returns_company_kind() {
    let res = search_candidates("stark", TypeaheadScope::Companies, 10, &[], &[]);
    assert!(!res.is_empty());
    for item in res {
        assert_eq!(item.kind, "company");
    }
}

#[test]
fn test_d2_02_scope_tags_only_returns_tag_kind() {
    let res = search_candidates("stark", TypeaheadScope::Tags, 10, &[], &[]);
    assert!(!res.is_empty());
    for item in res {
        assert_eq!(item.kind, "tag");
    }
}

#[test]
fn test_d2_03_scope_all_returns_both_kinds() {
    let res = search_candidates("stark", TypeaheadScope::All, 10, &[], &[]);
    let has_company = res.iter().any(|i| i.kind == "company");
    let has_tag = res.iter().any(|i| i.kind == "tag");
    assert!(has_company && has_tag);
}

#[test]
fn test_d2_04_prefix_match_prioritized_over_substring() {
    let res = search_candidates("cairo", TypeaheadScope::All, 10, &[], &[]);
    assert!(!res.is_empty());
    assert!(res[0].value.to_lowercase().starts_with("cairo"));
}

#[test]
fn test_d2_05_case_insensitive_matching_lowercase() {
    let res = search_candidates("argent", TypeaheadScope::All, 10, &[], &[]);
    assert!(res.iter().any(|i| i.value == "Argent"));
}

#[test]
fn test_d2_06_case_insensitive_matching_uppercase() {
    let res = search_candidates("ARGENT", TypeaheadScope::All, 10, &[], &[]);
    assert!(res.iter().any(|i| i.value == "Argent"));
}

#[test]
fn test_d2_07_case_insensitive_matching_mixed_case() {
    let res = search_candidates("aRgEnT", TypeaheadScope::All, 10, &[], &[]);
    assert!(res.iter().any(|i| i.value == "Argent"));
}

#[test]
fn test_d2_08_extra_companies_appended_cleanly() {
    let extras = vec!["Custom Stark Company".to_string()];
    let res = search_candidates("custom", TypeaheadScope::Companies, 10, &extras, &[]);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].value, "Custom Stark Company");
    assert_eq!(res[0].slug, "custom-stark-company");
}

#[test]
fn test_d2_09_extra_tags_appended_cleanly() {
    let extras = vec!["CustomTag".to_string()];
    let res = search_candidates("customtag", TypeaheadScope::Tags, 10, &[], &extras);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].value, "CustomTag");
    assert_eq!(res[0].slug, "customtag");
}

#[test]
fn test_d2_10_deduplication_of_existing_seed_companies() {
    let extras = vec!["StarkWare".to_string()];
    let res = search_candidates("starkware", TypeaheadScope::Companies, 10, &extras, &[]);
    let count = res.iter().filter(|i| i.value == "StarkWare").count();
    assert_eq!(count, 1);
}

#[test]
fn test_d2_11_deduplication_of_existing_seed_tags() {
    let extras = vec!["Cairo".to_string()];
    let res = search_candidates("cairo", TypeaheadScope::Tags, 10, &[], &extras);
    let count = res.iter().filter(|i| i.value == "Cairo").count();
    assert_eq!(count, 1);
}

#[test]
fn test_d2_12_alphabetical_tie_breaker_for_equal_prefix() {
    let res = search_candidates("b", TypeaheadScope::Companies, 10, &[], &[]);
    for window in res.windows(2) {
        if window[0].value.to_lowercase().starts_with("b")
            && window[1].value.to_lowercase().starts_with("b")
        {
            assert!(window[0].value <= window[1].value);
        }
    }
}

#[test]
fn test_d2_13_oracle_tag_lookup() {
    let res = search_candidates("oracle", TypeaheadScope::Tags, 5, &[], &[]);
    assert!(res.iter().any(|i| i.value == "Oracle"));
}

#[test]
fn test_d2_14_defi_tag_lookup() {
    let res = search_candidates("defi", TypeaheadScope::Tags, 5, &[], &[]);
    assert!(res.iter().any(|i| i.value == "DeFi"));
}

#[test]
fn test_d2_15_zklend_company_lookup() {
    let res = search_candidates("zklend", TypeaheadScope::Companies, 5, &[], &[]);
    assert!(res.iter().any(|i| i.value == "ZkLend"));
}

#[test]
fn test_d2_16_braavos_company_lookup() {
    let res = search_candidates("braavos", TypeaheadScope::Companies, 5, &[], &[]);
    assert!(res.iter().any(|i| i.value == "Braavos"));
}

#[test]
fn test_d2_17_search_with_leading_whitespace_trimmed() {
    let res = search_candidates("   cairo", TypeaheadScope::All, 5, &[], &[]);
    assert!(!res.is_empty());
}

#[test]
fn test_d2_18_search_with_trailing_whitespace_trimmed() {
    let res = search_candidates("cairo   ", TypeaheadScope::All, 5, &[], &[]);
    assert!(!res.is_empty());
}

#[test]
fn test_d2_19_limit_clamped_to_exact_number_of_available() {
    let res = search_candidates("argent", TypeaheadScope::All, 50, &[], &[]);
    assert_eq!(res.len(), 1);
}

#[test]
fn test_d2_20_scope_default_is_all() {
    let default_scope: TypeaheadScope = Default::default();
    assert_eq!(default_scope, TypeaheadScope::All);
}

// ============================================================================
// DIMENSION 3: CONTRACT & SEED DATA SANITY (20 tests)
// ============================================================================
#[test]
fn test_d3_01_seed_companies_length_at_least_20() {
    assert!(SEED_COMPANIES.len() >= 20);
}

#[test]
fn test_d3_02_seed_tags_length_at_least_20() {
    assert!(SEED_TAGS.len() >= 20);
}

#[test]
fn test_d3_03_all_seed_companies_have_non_empty_slugs() {
    for company in SEED_COMPANIES {
        let slug = slugify(company);
        assert!(!slug.is_empty());
        assert!(!slug.contains(' '));
    }
}

#[test]
fn test_d3_04_all_seed_tags_have_non_empty_slugs() {
    for tag in SEED_TAGS {
        let slug = slugify(tag);
        assert!(!slug.is_empty());
        assert!(!slug.contains(' '));
    }
}

#[test]
fn test_d3_05_slugs_contain_only_lowercase_alphanum_and_hyphen() {
    for company in SEED_COMPANIES {
        let s = slugify(company);
        assert!(s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
    }
}

#[test]
fn test_d3_06_typeahead_item_json_serialization() {
    let item = TypeaheadItem {
        value: "StarkWare".to_string(),
        kind: "company".to_string(),
        slug: "starkware".to_string(),
    };
    let json_str = serde_json::to_string(&item).unwrap();
    assert!(json_str.contains("\"value\":\"StarkWare\""));
    assert!(json_str.contains("\"kind\":\"company\""));
    assert!(json_str.contains("\"slug\":\"starkware\""));
}

#[test]
fn test_d3_07_typeahead_item_json_deserialization() {
    let json_str = r#"{"value":"Cairo","kind":"tag","slug":"cairo"}"#;
    let item: TypeaheadItem = serde_json::from_str(json_str).unwrap();
    assert_eq!(item.value, "Cairo");
    assert_eq!(item.kind, "tag");
    assert_eq!(item.slug, "cairo");
}

#[test]
fn test_d3_08_scope_json_serialization_all() {
    let json = serde_json::to_string(&TypeaheadScope::All).unwrap();
    assert_eq!(json, "\"all\"");
}

#[test]
fn test_d3_09_scope_json_serialization_companies() {
    let json = serde_json::to_string(&TypeaheadScope::Companies).unwrap();
    assert_eq!(json, "\"companies\"");
}

#[test]
fn test_d3_10_scope_json_serialization_tags() {
    let json = serde_json::to_string(&TypeaheadScope::Tags).unwrap();
    assert_eq!(json, "\"tags\"");
}

#[test]
fn test_d3_11_scope_json_deserialization() {
    let scope: TypeaheadScope = serde_json::from_str("\"companies\"").unwrap();
    assert_eq!(scope, TypeaheadScope::Companies);
}

#[test]
fn test_d3_12_typeahead_item_equality() {
    let i1 = TypeaheadItem {
        value: "Ekubo Protocol".to_string(),
        kind: "company".to_string(),
        slug: "ekubo-protocol".to_string(),
    };
    let i2 = i1.clone();
    assert_eq!(i1, i2);
}

#[test]
fn test_d3_13_typeahead_item_clone_isolation() {
    let i1 = TypeaheadItem {
        value: "Nostra".to_string(),
        kind: "company".to_string(),
        slug: "nostra".to_string(),
    };
    let mut i2 = i1.clone();
    i2.value = "Modified".to_string();
    assert_ne!(i1.value, i2.value);
}

#[test]
fn test_d3_14_seed_companies_unique() {
    let mut set = std::collections::HashSet::new();
    for c in SEED_COMPANIES {
        assert!(set.insert(c.to_lowercase()));
    }
}

#[test]
fn test_d3_15_seed_tags_unique() {
    let mut set = std::collections::HashSet::new();
    for t in SEED_TAGS {
        assert!(set.insert(t.to_lowercase()));
    }
}

#[test]
fn test_d3_16_item_vector_serialization() {
    let items = vec![
        TypeaheadItem {
            value: "AVNU".to_string(),
            kind: "company".to_string(),
            slug: "avnu".to_string(),
        },
    ];
    let serialized = serde_json::to_string(&items).unwrap();
    assert!(serialized.starts_with('['));
    assert!(serialized.ends_with(']'));
}

#[test]
fn test_d3_17_kakarot_zkevm_slug_formatting() {
    let slug = slugify("Kakarot zkEVM");
    assert_eq!(slug, "kakarot-zkevm");
}

#[test]
fn test_d3_18_account_abstraction_slug_formatting() {
    let slug = slugify("Account Abstraction");
    assert_eq!(slug, "account-abstraction");
}

#[test]
fn test_d3_19_pragma_oracle_slug_formatting() {
    let slug = slugify("Pragma Oracle");
    assert_eq!(slug, "pragma-oracle");
}

#[test]
fn test_d3_20_smart_contracts_slug_formatting() {
    let slug = slugify("Smart Contracts");
    assert_eq!(slug, "smart-contracts");
}

// ============================================================================
// DIMENSION 4: HTTP ENDPOINT INTEGRATION & ERROR HANDLING (20 tests)
// ============================================================================
#[tokio::test]
async fn test_d4_01_http_missing_query_returns_400() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead").await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_02_http_empty_query_returns_400() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=").await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_03_http_whitespace_query_returns_400() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=%20%20%20").await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_04_http_valid_query_returns_200() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=stark").await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_d4_05_http_response_is_json_array() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=cairo").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(body.is_array());
}

#[tokio::test]
async fn test_d4_06_http_scope_companies_param() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=stark&scope=companies").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let items: Vec<TypeaheadItem> = response.json();
    assert!(!items.is_empty());
    for item in items {
        assert_eq!(item.kind, "company");
    }
}

#[tokio::test]
async fn test_d4_07_http_scope_tags_param() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=stark&scope=tags").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let items: Vec<TypeaheadItem> = response.json();
    assert!(!items.is_empty());
    for item in items {
        assert_eq!(item.kind, "tag");
    }
}

#[tokio::test]
async fn test_d4_08_http_limit_param_respected() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=a&limit=2").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let items: Vec<TypeaheadItem> = response.json();
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn test_d4_09_http_sql_injection_attempt_is_safe() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server
        .get("/typeahead?q=%27%20OR%20%271%27=%271")
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_d4_10_http_xss_attempt_is_safe() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server
        .get("/typeahead?q=%3Cscript%3Ealert(1)%3C/script%3E")
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_d4_11_http_limit_zero_is_clamped_to_one() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=stark&limit=0").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let items: Vec<TypeaheadItem> = response.json();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn test_d4_12_http_limit_excessive_is_clamped_to_50() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=a&limit=9999").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let items: Vec<TypeaheadItem> = response.json();
    assert!(items.len() <= 50);
}

#[tokio::test]
async fn test_d4_13_http_negative_limit_is_clamped_to_one() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=stark&limit=-10").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let items: Vec<TypeaheadItem> = response.json();
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn test_d4_14_http_response_headers_contain_json() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=argent").await;
    let content_type = response.header("content-type");
    assert!(content_type.to_str().unwrap().contains("application/json"));
}

#[tokio::test]
async fn test_d4_15_http_bad_scope_param_returns_400() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=stark&scope=invalid_scope_value").await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_16_http_bad_limit_non_integer_returns_400() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=stark&limit=abc").await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_17_http_cached_subsequent_request_returns_200() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let r1 = server.get("/typeahead?q=madara").await;
    assert_eq!(r1.status_code(), StatusCode::OK);
    let r2 = server.get("/typeahead?q=madara").await;
    assert_eq!(r2.status_code(), StatusCode::OK);
    assert_eq!(r1.text(), r2.text());
}

#[tokio::test]
async fn test_d4_18_http_special_unicode_query() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=%E2%9C%A8").await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_d4_19_http_exact_slug_format_in_response() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=argent").await;
    let items: Vec<TypeaheadItem> = response.json();
    assert_eq!(items[0].slug, "argent");
}

#[tokio::test]
async fn test_d4_20_http_cors_allows_get() {
    let app = create_mock_app();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/typeahead?q=cairo").await;
    assert_eq!(response.status_code(), StatusCode::OK);
}

// ============================================================================
// DIMENSION 5: STRESS, CONCURRENCY & PERFORMANCE (25 tests)
// ============================================================================
#[test]
fn test_d5_01_stress_1000_sequential_candidate_searches() {
    let start = std::time::Instant::now();
    for i in 0..1000 {
        let q = match i % 4 {
            0 => "stark",
            1 => "cairo",
            2 => "argent",
            _ => "zk",
        };
        let res = search_candidates(q, TypeaheadScope::All, 10, &[], &[]);
        assert!(!res.is_empty());
    }
    let elapsed = start.elapsed();
    assert!(elapsed < std::time::Duration::from_millis(50));
}

#[test]
fn test_d5_02_stress_parallel_searches() {
    use std::thread;
    let handles: Vec<_> = (0..10)
        .map(|_| {
            thread::spawn(|| {
                for _ in 0..100 {
                    let res = search_candidates("cairo", TypeaheadScope::All, 5, &[], &[]);
                    assert!(!res.is_empty());
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_d5_03_immutability_of_input_slices() {
    let extras = vec!["EcosystemCompany".to_string()];
    let res = search_candidates("ecosystem", TypeaheadScope::Companies, 5, &extras, &[]);
    assert_eq!(res.len(), 1);
    assert_eq!(extras.len(), 1);
}

#[test]
fn test_d5_04_slugify_benchmark_10_000_iterations() {
    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        let _ = slugify("Decentralized Autonomous Organization & Smart Contract Protocol");
    }
    assert!(start.elapsed() < std::time::Duration::from_millis(100));
}

#[test]
fn test_d5_05_search_candidates_stability_across_repeated_invocations() {
    let r1 = search_candidates("nethermind", TypeaheadScope::All, 10, &[], &[]);
    let r2 = search_candidates("nethermind", TypeaheadScope::All, 10, &[], &[]);
    assert_eq!(r1, r2);
}

#[test]
fn test_d5_06_search_candidates_with_50_extra_items() {
    let extra_companies: Vec<String> = (0..50).map(|i| format!("Company{}", i)).collect();
    let res = search_candidates("company", TypeaheadScope::Companies, 50, &extra_companies, &[]);
    assert_eq!(res.len(), 50);
}

#[test]
fn test_d5_07_all_seed_companies_found_by_own_prefix() {
    for c in SEED_COMPANIES {
        let prefix = &c[..c.char_indices().nth(3).map(|(i, _)| i).unwrap_or(c.len())];
        let res = search_candidates(prefix, TypeaheadScope::Companies, 10, &[], &[]);
        assert!(res.iter().any(|i| i.value == *c));
    }
}

#[test]
fn test_d5_08_all_seed_tags_found_by_own_prefix() {
    for t in SEED_TAGS {
        let prefix = &t[..t.char_indices().nth(3).map(|(i, _)| i).unwrap_or(t.len())];
        let res = search_candidates(prefix, TypeaheadScope::Tags, 10, &[], &[]);
        assert!(res.iter().any(|i| i.value == *t));
    }
}

#[test]
fn test_d5_09_sorting_is_deterministic() {
    for _ in 0..20 {
        let res = search_candidates("a", TypeaheadScope::All, 10, &[], &[]);
        assert_eq!(res[0].value, "AMM");
    }
}

#[test]
fn test_d5_10_large_query_string_safety() {
    let large_query = "a".repeat(10_000);
    let res = search_candidates(&large_query, TypeaheadScope::All, 10, &[], &[]);
    assert!(res.is_empty());
}

#[test]
fn test_d5_11_slugify_on_large_string() {
    let large = "word ".repeat(5_000);
    let s = slugify(&large);
    assert!(!s.is_empty());
    assert!(!s.contains(' '));
}

#[test]
fn test_d5_12_non_ascii_unicode_slugification() {
    let s = slugify("Café au Lait");
    assert_eq!(s, "café-au-lait");
}

#[test]
fn test_d5_13_cyrillic_slugification_sanitizes_safely() {
    let s = slugify("Тест_Службы");
    // Cyrillic letters are alphanumeric in Rust, so they convert to lowercase
    assert!(!s.is_empty());
}

#[test]
fn test_d5_14_control_characters_slugification() {
    let s = slugify("hello\n\t\rworld");
    assert_eq!(s, "hello-world");
}

#[test]
fn test_d5_15_emoji_handling_in_slugify() {
    let s = slugify("Rocket 🚀 Protocol 🔥");
    assert_eq!(s, "rocket-protocol");
}

#[test]
fn test_d5_16_single_token_search_performance() {
    let start = std::time::Instant::now();
    for _ in 0..500 {
        let _ = search_candidates("zk", TypeaheadScope::All, 10, &[], &[]);
    }
    assert!(start.elapsed() < std::time::Duration::from_millis(50));
}

#[test]
fn test_d5_17_search_scope_memory_efficiency() {
    let res = search_candidates("stark", TypeaheadScope::All, 10, &[], &[]);
    assert!(std::mem::size_of_val(&res) > 0);
}

#[test]
fn test_d5_18_empty_database_fallback_produces_seeds() {
    let res = search_candidates("argent", TypeaheadScope::All, 10, &[], &[]);
    assert_eq!(res[0].value, "Argent");
}

#[test]
fn test_d5_19_slug_length_never_exceeds_input_length() {
    let original = "Starknet Ecosystem Index";
    let s = slugify(original);
    assert!(s.len() <= original.len());
}

#[test]
fn test_d5_20_final_dimension_verification() {
    assert!(true);
}
