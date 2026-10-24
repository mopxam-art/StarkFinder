use axum::http::{StatusCode, header};
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::models::retraction::{
    AdminRetractionAuditResponse, CompanyFeedResponse, RetractableReview,
    RetractReviewRequest, RetractReviewResponse,
};
use backend::services::retraction_service::{
    RETRACTION_SERVICE, anonymize_wallet, canonical_wallet,
};
use chrono::Utc;
use uuid::Uuid;

fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-retraction") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-retraction") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

fn create_mock_review(company_slug: &str, author_wallet: &str, title: &str) -> RetractableReview {
    let now = Utc::now();
    RetractableReview {
        id: Uuid::new_v4(),
        company_slug: company_slug.to_string(),
        author_wallet: canonical_wallet(author_wallet),
        title: title.to_string(),
        content: "Detailed review content with insights.".to_string(),
        rating: 5,
        tags: vec!["engineering".to_string(), "culture".to_string()],
        created_at: now,
        updated_at: now,
        deleted_at: None,
        deleted_reason: None,
    }
}

// =========================================================================
// SECTION 1: UNIT TESTS - HELPERS & MODELS
// =========================================================================

#[test]
fn test_unit_is_deleted_helper() {
    let mut rev = create_mock_review("stark-tech", "0x10", "Great place");
    assert!(!rev.is_deleted());

    rev.deleted_at = Some(Utc::now());
    assert!(rev.is_deleted());
}

#[test]
fn test_unit_anonymize_wallet() {
    let wallet_long = "0x1234567890abcdef1234567890abcdef";
    let anon_long = anonymize_wallet(wallet_long);
    assert!(anon_long.starts_with("0x12..."));
    assert!(anon_long.contains("..."));

    let wallet_short = "0x1";
    let anon_short = anonymize_wallet(wallet_short);
    assert!(anon_short.contains("..."));
}

// =========================================================================
// SECTION 2: INTEGRATION TESTS - DELETE /posts/{id} & 410 GONE SEMANTICS
// =========================================================================

#[tokio::test]
async fn test_retract_review_success_by_author() {
    let app = setup_test_app();
    let author_wallet = "0x101";
    let review = create_mock_review("argent-tech", author_wallet, "Solid Wallet Engineering");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let token = create_auth_token(author_wallet);
    let payload = RetractReviewRequest {
        reason: Some("Personal request to remove".to_string()),
    };

    let res = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: RetractReviewResponse = res.json();
    assert_eq!(body.review_id, review_id);

    // Verify GET /posts/{id} now returns 410 Gone with {"error": "Review retracted", "code": 410}
    let get_res = app.get(&format!("/posts/{}", review_id)).await;
    assert_eq!(get_res.status_code(), StatusCode::GONE);
    let get_json: serde_json::Value = get_res.json();
    assert_eq!(get_json["error"], "Review retracted");
    assert_eq!(get_json["code"], 410);
}

#[tokio::test]
async fn test_retract_review_without_reason_body() {
    let app = setup_test_app();
    let author_wallet = "0x102";
    let review = create_mock_review("braavos-tech", author_wallet, "Security Review");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let token = create_auth_token(author_wallet);

    let res = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_retract_review_forbidden_non_author() {
    let app = setup_test_app();
    let author_wallet = "0x103";
    let attacker_wallet = "0x104";
    let review = create_mock_review("starkware-tech", author_wallet, "STARK Innovators");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let token = create_auth_token(attacker_wallet);

    let res = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&RetractReviewRequest {
            reason: Some("Unauthorized deletion".to_string()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = res.json();
    assert_eq!(json["error"], "only review author can retract this review");

    // Review remains active
    let get_res = app.get(&format!("/posts/{}", review_id)).await;
    assert_eq!(get_res.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_retract_review_unauthorized_missing_jwt() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let res = app.delete(&format!("/posts/{}", review_id)).await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_retract_review_unauthorized_invalid_jwt() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let res = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, "Bearer invalid-tampered-token")
        .await;

    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_retract_review_404_nonexistent() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();
    let token = create_auth_token("0x105");

    let res = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_retract_review_repeat_delete_returns_410() {
    let app = setup_test_app();
    let author_wallet = "0x106";
    let review = create_mock_review("avnu-tech", author_wallet, "DEX Aggregator Review");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let token = create_auth_token(author_wallet);

    // First retraction
    let res_1 = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res_1.status_code(), StatusCode::OK);

    // Second retraction attempt
    let res_2 = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res_2.status_code(), StatusCode::GONE);
    let json: serde_json::Value = res_2.json();
    assert_eq!(json["code"], 410);
}

#[tokio::test]
async fn test_get_active_review_returns_200() {
    let app = setup_test_app();
    let author_wallet = "0x107";
    let review = create_mock_review("cartridge-tech", author_wallet, "Dojo Gaming Engineering");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let res = app.get(&format!("/posts/{}", review_id)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: RetractableReview = res.json();
    assert_eq!(body.id, review_id);
    assert_eq!(body.title, "Dojo Gaming Engineering");
    assert!(!body.is_deleted());
}

#[tokio::test]
async fn test_get_review_404_nonexistent() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let res = app.get(&format!("/posts/{}", review_id)).await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

// =========================================================================
// SECTION 3: PUBLIC FEED FILTERING & COUNT INTEGRATION
// =========================================================================

#[tokio::test]
async fn test_public_feed_excludes_retracted_reviews_and_decrements_count() {
    let app = setup_test_app();
    let slug = "ekubo-protocol";
    let author_1 = "0x108";
    let author_2 = "0x109";

    let rev_1 = create_mock_review(slug, author_1, "Review 1 - AMM Math");
    let rev_2 = create_mock_review(slug, author_2, "Review 2 - Concentrated Liquidity");
    let id_1 = rev_1.id;
    let id_2 = rev_2.id;

    RETRACTION_SERVICE.register_review(rev_1);
    RETRACTION_SERVICE.register_review(rev_2);

    // Initial feed check: 2 reviews
    let feed_1: CompanyFeedResponse = app.get(&format!("/feed/{}", slug)).await.json();
    assert_eq!(feed_1.total_reviews, 2);
    assert_eq!(feed_1.reviews.len(), 2);

    // Author 1 retracts Review 1
    let token_1 = create_auth_token(author_1);
    let del_res = app
        .delete(&format!("/posts/{}", id_1))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token_1))
        .await;
    assert_eq!(del_res.status_code(), StatusCode::OK);

    // Subsequent feed check: 1 review remaining, id_1 excluded
    let feed_2: CompanyFeedResponse = app.get(&format!("/feed/{}", slug)).await.json();
    assert_eq!(feed_2.total_reviews, 1);
    assert_eq!(feed_2.reviews.len(), 1);
    assert_eq!(feed_2.reviews[0].id, id_2);
}

#[tokio::test]
async fn test_public_feed_empty_when_all_retracted() {
    let app = setup_test_app();
    let slug = "solo-company";
    let author = "0x10a";
    let rev = create_mock_review(slug, author, "Only Review");
    let id = rev.id;
    RETRACTION_SERVICE.register_review(rev);

    let token = create_auth_token(author);
    let _ = app
        .delete(&format!("/posts/{}", id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    let feed: CompanyFeedResponse = app.get(&format!("/feed/{}", slug)).await.json();
    assert_eq!(feed.total_reviews, 0);
    assert!(feed.reviews.is_empty());
}

#[tokio::test]
async fn test_multi_company_feed_isolation() {
    let app = setup_test_app();
    let comp_a = "company-alpha";
    let comp_b = "company-beta";

    let rev_a = create_mock_review(comp_a, "0x10b", "Alpha review");
    let rev_b = create_mock_review(comp_b, "0x10c", "Beta review");
    let id_a = rev_a.id;
    let id_b = rev_b.id;

    RETRACTION_SERVICE.register_review(rev_a);
    RETRACTION_SERVICE.register_review(rev_b);

    // Retract Alpha review
    let token_a = create_auth_token("0x10b");
    let _ = app
        .delete(&format!("/posts/{}", id_a))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token_a))
        .await;

    // Beta feed remains intact
    let feed_b: CompanyFeedResponse = app.get(&format!("/feed/{}", comp_b)).await.json();
    assert_eq!(feed_b.total_reviews, 1);
    assert_eq!(feed_b.reviews[0].id, id_b);
}

// =========================================================================
// SECTION 4: ADMIN PRIVILEGES & AUDIT LOG WITH PRIVACY ANONYMIZATION
// =========================================================================

#[tokio::test]
async fn test_admin_can_retract_any_review() {
    let app = setup_test_app();
    let admin_wallet = "0x10d";
    let author_wallet = "0x10e";

    RETRACTION_SERVICE.add_admin(admin_wallet);

    let review = create_mock_review("moderated-company", author_wallet, "Spammy review");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let admin_token = create_auth_token(admin_wallet);

    let res = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .json(&RetractReviewRequest {
            reason: Some("Moderator intervention: violation of TOS".to_string()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);

    // Now 410 Gone
    let get_res = app.get(&format!("/posts/{}", review_id)).await;
    assert_eq!(get_res.status_code(), StatusCode::GONE);
}

#[tokio::test]
async fn test_admin_retractions_audit_success_and_anonymization() {
    let app = setup_test_app();
    let admin_wallet = "0x10f";
    let author_wallet = "0x110";

    RETRACTION_SERVICE.add_admin(admin_wallet);

    let review = create_mock_review("audit-company", author_wallet, "Retracted for privacy");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let author_token = create_auth_token(author_wallet);
    let _ = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", author_token))
        .json(&RetractReviewRequest {
            reason: Some("Author NDA confidentiality".to_string()),
        })
        .await;

    let admin_token = create_auth_token(admin_wallet);
    let res = app
        .get("/admin/retractions")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let audit_resp: AdminRetractionAuditResponse = res.json();
    assert!(!audit_resp.audits.is_empty());

    let entry = audit_resp.audits.iter().find(|a| a.review_id == review_id).unwrap();
    assert_eq!(entry.reason, Some("Author NDA confidentiality".to_string()));
    // Author wallet is anonymized / masked
    assert!(entry.anonymized_author.contains("..."));
}

#[tokio::test]
async fn test_admin_retractions_forbidden_for_non_admin() {
    let app = setup_test_app();
    let non_admin_wallet = "0x111";
    let token = create_auth_token(non_admin_wallet);

    let res = app
        .get("/admin/retractions")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_admin_retractions_unauthorized() {
    let app = setup_test_app();
    let res = app.get("/admin/retractions").await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// SECTION 5: VALIDATION, EDGE CASES & NORMALIZATION
// =========================================================================

#[tokio::test]
async fn test_invalid_uuid_returns_400_bad_request() {
    let app = setup_test_app();
    let token = create_auth_token("0x112");

    let del_res = app
        .delete("/posts/not-a-valid-uuid")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(del_res.status_code(), StatusCode::BAD_REQUEST);

    let get_res = app.get("/posts/not-a-valid-uuid").await;
    assert_eq!(get_res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_starknet_address_case_insensitivity_and_leading_zeros() {
    let app = setup_test_app();
    let author_wallet_short = "0x113";
    let author_wallet_long = "0x0000000000000000000000000000000000000000000000000000000000000113";

    let review = create_mock_review("stark-wallet", author_wallet_short, "Canonical wallet check");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    // Delete using token created with long form
    let token = create_auth_token(author_wallet_long);
    let res = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_public_feed_pagination() {
    let app = setup_test_app();
    let slug = "pagination-corp";

    for i in 0..5 {
        let rev = create_mock_review(slug, &format!("0x{:x}", 0x200 + i), &format!("Review {}", i));
        RETRACTION_SERVICE.register_review(rev);
    }

    let p1: CompanyFeedResponse = app.get(&format!("/feed/{}?limit=2&offset=0", slug)).await.json();
    let p2: CompanyFeedResponse = app.get(&format!("/feed/{}?limit=2&offset=2", slug)).await.json();

    assert_eq!(p1.total_reviews, 5);
    assert_eq!(p1.reviews.len(), 2);
    assert_eq!(p2.reviews.len(), 2);
    assert_ne!(p1.reviews[0].id, p2.reviews[0].id);
}

#[tokio::test]
async fn test_retract_audit_fields_verified() {
    let author_wallet = "0x250";
    let review = create_mock_review("audit-fields-corp", author_wallet, "Audit field check");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let reason_text = "Retracting due to duplicate entry";
    let audit = RETRACTION_SERVICE
        .retract_review(review_id, author_wallet, Some(reason_text.to_string()))
        .unwrap();

    assert_eq!(audit.review_id, review_id);
    assert_eq!(audit.retracted_by_wallet, canonical_wallet(author_wallet));
    assert_eq!(audit.reason, Some(reason_text.to_string()));
    assert!(audit.retracted_at <= Utc::now());
}

#[tokio::test]
async fn test_repeated_delete_returns_410_persistently() {
    let app = setup_test_app();
    let author = "0x251";
    let review = create_mock_review("persistent-410", author, "Repeat check");
    let review_id = review.id;
    RETRACTION_SERVICE.register_review(review);

    let token = create_auth_token(author);

    // Initial delete
    let res1 = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res1.status_code(), StatusCode::OK);

    // 2nd delete -> 410
    let res2 = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res2.status_code(), StatusCode::GONE);

    // 3rd delete -> 410
    let res3 = app
        .delete(&format!("/posts/{}", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res3.status_code(), StatusCode::GONE);
}

#[tokio::test]
async fn test_admin_audit_pagination_with_offset() {
    let app = setup_test_app();
    let admin = "0x252";
    RETRACTION_SERVICE.add_admin(admin);

    for i in 0..4 {
        let rev = create_mock_review("admin-page-corp", &format!("0x{:x}", 0x300 + i), &format!("Review {}", i));
        let id = rev.id;
        RETRACTION_SERVICE.register_review(rev);
        let _ = RETRACTION_SERVICE.retract_review(id, &format!("0x{:x}", 0x300 + i), None);
    }

    let admin_token = create_auth_token(admin);
    let p1: AdminRetractionAuditResponse = app
        .get("/admin/retractions?limit=2&offset=0")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .await
        .json();
    let p2: AdminRetractionAuditResponse = app
        .get("/admin/retractions?limit=2&offset=2")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", admin_token))
        .await
        .json();

    assert!(p1.total >= 4);
    assert_eq!(p1.audits.len(), 2);
    assert_eq!(p2.audits.len(), 2);
    assert_ne!(p1.audits[0].id, p2.audits[0].id);
}

#[tokio::test]
async fn test_public_feed_offset_beyond_total() {
    let app = setup_test_app();
    let slug = "offset-beyond";
    let rev = create_mock_review(slug, "0x253", "Single review");
    RETRACTION_SERVICE.register_review(rev);

    let res: CompanyFeedResponse = app.get(&format!("/feed/{}?offset=100", slug)).await.json();
    assert_eq!(res.total_reviews, 1);
    assert!(res.reviews.is_empty());
}

#[tokio::test]
async fn test_admin_retractions_audit_fetch_success() {
    let app = setup_test_app();
    let admin_wallet = "0x254";
    RETRACTION_SERVICE.add_admin(admin_wallet);

    let token = create_auth_token(admin_wallet);
    let res = app
        .get("/admin/retractions")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let resp: AdminRetractionAuditResponse = res.json();
    assert!(resp.total >= resp.audits.len());
}


