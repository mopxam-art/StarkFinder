use axum::http::{StatusCode, header};
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::models::company_response::{
    CompanyResponse, CompanyResponseAudit, CreateOrUpdateResponseDto,
};
use backend::services::company_response_service::{
    COMPANY_RESPONSE_SERVICE, canonical_slug, canonical_wallet, sanitize_response_text,
};
use uuid::Uuid;

fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-company-responses") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-company-responses") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

// =========================================================================
// SECTION 1: UNIT TESTS - SERVICE & OWNERSHIP LOGIC
// =========================================================================

#[test]
fn test_owner_registration_and_verification() {
    let slug = "stark-inc-01";
    let owner_wallet = "0x101";
    let other_wallet = "0x102";

    assert!(!COMPANY_RESPONSE_SERVICE.is_company_owner(slug, owner_wallet));

    COMPANY_RESPONSE_SERVICE.add_company_owner(slug, owner_wallet);
    assert!(COMPANY_RESPONSE_SERVICE.is_company_owner(slug, owner_wallet));
    assert!(COMPANY_RESPONSE_SERVICE.is_company_owner(" STARK-INC-01 ", "0x000101"));
    assert!(!COMPANY_RESPONSE_SERVICE.is_company_owner(slug, other_wallet));

    let removed = COMPANY_RESPONSE_SERVICE.remove_company_owner(slug, owner_wallet);
    assert!(removed);
    assert!(!COMPANY_RESPONSE_SERVICE.is_company_owner(slug, owner_wallet));
}

#[test]
fn test_canonical_wallet_normalization() {
    let felt_wallet = "0x1";
    let normalized = canonical_wallet(felt_wallet);
    assert!(normalized.starts_with("0x"));
    assert_eq!(normalized, canonical_wallet("0x0000000000000000000000000000000000000000000000000000000000000001"));

    let plain_wallet = "non-hex-wallet";
    assert_eq!(canonical_wallet(plain_wallet), "non-hex-wallet");
}

#[test]
fn test_canonical_slug() {
    assert_eq!(canonical_slug("  Acme-Corp  "), "acme-corp");
}

#[test]
fn test_sanitization_pii_email() {
    let input = "Contact our executive team at support@company.com or ceo.doe@enterprise.org immediately.";
    let sanitized = sanitize_response_text(input);
    assert!(!sanitized.contains("support@company.com"));
    assert!(!sanitized.contains("ceo.doe@enterprise.org"));
    assert!(sanitized.contains("[REDACTED_EMAIL]"));
}

#[test]
fn test_sanitization_pii_phone() {
    let input = "Call our hotline at +1 (555) 234-5678 or 555-876-5432 to discuss.";
    let sanitized = sanitize_response_text(input);
    assert!(!sanitized.contains("555"));
    assert!(sanitized.contains("[REDACTED_PHONE]"));
}

#[test]
fn test_sanitization_pii_credit_card() {
    let input = "Refund sent to 4532 1234 5678 9012.";
    let sanitized = sanitize_response_text(input);
    assert!(!sanitized.contains("4532"));
    assert!(sanitized.contains("[REDACTED_CARD]"));
}

#[test]
fn test_sanitization_pii_private_key() {
    let input = "Internal secret key: 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.";
    let sanitized = sanitize_response_text(input);
    assert!(!sanitized.contains("0123456789abcdef"));
    assert!(sanitized.contains("[REDACTED_KEY]"));
}

#[test]
fn test_sanitization_xss_script_tags() {
    let input = "Hello <script type=\"text/javascript\">alert('XSS');</script> world!";
    let sanitized = sanitize_response_text(input);
    assert_eq!(sanitized, "Hello  world!");
    assert!(!sanitized.contains("alert"));
}

#[test]
fn test_sanitization_xss_dangerous_tags_and_events() {
    let input = "Check this <iframe src=\"evil.com\"></iframe> and <div onload=\"steal()\">content</div> <a href=\"javascript:alert(1)\">link</a>";
    let sanitized = sanitize_response_text(input);
    assert!(!sanitized.contains("iframe"));
    assert!(!sanitized.contains("onload"));
    assert!(!sanitized.contains("javascript:"));
}

// =========================================================================
// SECTION 2: INTEGRATION TESTS - HTTP ENDPOINTS
// =========================================================================

#[tokio::test]
async fn test_post_response_success_authorized_owner() {
    let app = setup_test_app();
    let company_slug = "company-corp-01";
    let owner_wallet = "0x201";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    let payload = CreateOrUpdateResponseDto {
        text: "Thank you for the constructive feedback. We have addressed the issue.".to_string(),
        company_slug: None,
    };

    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: CompanyResponse = res.json();
    assert_eq!(body.review_id, review_id);
    assert_eq!(body.company_slug, company_slug);
    assert_eq!(body.owner_wallet, canonical_wallet(owner_wallet));
    assert_eq!(body.text, payload.text);
}

#[tokio::test]
async fn test_post_response_forbidden_non_owner() {
    let app = setup_test_app();
    let company_slug = "company-corp-02";
    let owner_wallet = "0x202";
    let non_owner_wallet = "0x203";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(non_owner_wallet);

    let payload = CreateOrUpdateResponseDto {
        text: "Attempting unauthorized response.".to_string(),
        company_slug: None,
    };

    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
    let json: serde_json::Value = res.json();
    assert_eq!(
        json["error"],
        "only verified company owners can publish official responses"
    );
}

#[tokio::test]
async fn test_post_response_unauthorized_missing_jwt() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let payload = CreateOrUpdateResponseDto {
        text: "Unauthenticated response attempt.".to_string(),
        company_slug: Some("company-03".to_string()),
    };

    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .json(&payload)
        .await;

    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_post_response_unauthorized_invalid_jwt() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let payload = CreateOrUpdateResponseDto {
        text: "Invalid token response attempt.".to_string(),
        company_slug: Some("company-04".to_string()),
    };

    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, "Bearer invalid-tampered-token")
        .json(&payload)
        .await;

    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_post_response_updates_existing_and_creates_audit_trail() {
    let app = setup_test_app();
    let company_slug = "company-corp-05";
    let owner_wallet = "0x205";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    // Initial post
    let initial_text = "Initial official statement from management.";
    let payload_1 = CreateOrUpdateResponseDto {
        text: initial_text.to_string(),
        company_slug: None,
    };
    let res_1 = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload_1)
        .await;
    assert_eq!(res_1.status_code(), StatusCode::OK);
    let initial_resp: CompanyResponse = res_1.json();

    // Secondary post (update)
    let updated_text = "Updated official statement after internal review.";
    let payload_2 = CreateOrUpdateResponseDto {
        text: updated_text.to_string(),
        company_slug: None,
    };
    let res_2 = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload_2)
        .await;
    assert_eq!(res_2.status_code(), StatusCode::OK);
    let updated_resp: CompanyResponse = res_2.json();

    // 1:1 check: same ID, updated text
    assert_eq!(updated_resp.id, initial_resp.id);
    assert_eq!(updated_resp.text, updated_text);

    // Verify audit trail via GET /posts/{id}/response/audit
    let audit_res = app
        .get(&format!("/posts/{}/response/audit", review_id))
        .await;
    assert_eq!(audit_res.status_code(), StatusCode::OK);
    let audits: Vec<CompanyResponseAudit> = audit_res.json();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].response_id, initial_resp.id);
    assert_eq!(audits[0].previous_text, initial_text);
    assert_eq!(audits[0].new_text, updated_text);
    assert_eq!(audits[0].editor_wallet, canonical_wallet(owner_wallet));
}

#[tokio::test]
async fn test_post_response_multiple_updates_accumulate_audit() {
    let app = setup_test_app();
    let company_slug = "company-corp-06";
    let owner_wallet = "0x206";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    let v1 = "Version 1 statement.";
    let v2 = "Version 2 statement with clarifications.";
    let v3 = "Version 3 finalized resolution.";

    let _ = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: v1.to_string(),
            company_slug: None,
        })
        .await;

    let _ = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: v2.to_string(),
            company_slug: None,
        })
        .await;

    let _ = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: v3.to_string(),
            company_slug: None,
        })
        .await;

    let audit_res = app
        .get(&format!("/posts/{}/response/audit", review_id))
        .await;
    assert_eq!(audit_res.status_code(), StatusCode::OK);
    let audits: Vec<CompanyResponseAudit> = audit_res.json();
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[0].previous_text, v1);
    assert_eq!(audits[0].new_text, v2);
    assert_eq!(audits[1].previous_text, v2);
    assert_eq!(audits[1].new_text, v3);
}

#[tokio::test]
async fn test_post_response_co_owner_can_update() {
    let app = setup_test_app();
    let company_slug = "company-corp-07";
    let owner_1 = "0x207";
    let owner_2 = "0x208";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_1);
    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_2);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token_1 = create_auth_token(owner_1);
    let token_2 = create_auth_token(owner_2);

    // Owner 1 posts
    let res_1 = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token_1))
        .json(&CreateOrUpdateResponseDto {
            text: "First response from founder.".to_string(),
            company_slug: None,
        })
        .await;
    assert_eq!(res_1.status_code(), StatusCode::OK);

    // Co-owner (Owner 2) updates
    let res_2 = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token_2))
        .json(&CreateOrUpdateResponseDto {
            text: "Follow up from co-founder.".to_string(),
            company_slug: None,
        })
        .await;
    assert_eq!(res_2.status_code(), StatusCode::OK);

    // Audit trail should record Owner 2 as editor
    let audits: Vec<CompanyResponseAudit> = app
        .get(&format!("/posts/{}/response/audit", review_id))
        .await
        .json();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].editor_wallet, canonical_wallet(owner_2));
}

#[tokio::test]
async fn test_post_response_rejects_empty_or_whitespace_text() {
    let app = setup_test_app();
    let company_slug = "company-corp-08";
    let owner_wallet = "0x209";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    // Empty text
    let res_empty = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: "".to_string(),
            company_slug: None,
        })
        .await;
    assert_eq!(res_empty.status_code(), StatusCode::BAD_REQUEST);

    // Whitespace only
    let res_ws = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: "   \n\t   ".to_string(),
            company_slug: None,
        })
        .await;
    assert_eq!(res_ws.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_post_response_rejects_exceeding_max_5000_chars() {
    let app = setup_test_app();
    let company_slug = "company-corp-09";
    let owner_wallet = "0x20a";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    let oversized_text = "a".repeat(5001);
    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: oversized_text,
            company_slug: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = res.json();
    assert!(json["error"].as_str().unwrap().contains("5000"));
}

#[tokio::test]
async fn test_post_response_accepts_exactly_5000_chars() {
    let app = setup_test_app();
    let company_slug = "company-corp-10";
    let owner_wallet = "0x20b";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    let boundary_text = "x".repeat(5000);
    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: boundary_text.clone(),
            company_slug: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: CompanyResponse = res.json();
    assert_eq!(body.text.len(), 5000);
}

#[tokio::test]
async fn test_post_response_sanitizes_pii_and_xss_in_body() {
    let app = setup_test_app();
    let company_slug = "company-corp-11";
    let owner_wallet = "0x20c";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    let raw_text = "Please reach out to support@company.com or call 555-432-1234. <script>alert(1)</script>";
    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: raw_text.to_string(),
            company_slug: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: CompanyResponse = res.json();
    assert!(!body.text.contains("support@company.com"));
    assert!(!body.text.contains("555-432-1234"));
    assert!(!body.text.contains("<script>"));
    assert!(body.text.contains("[REDACTED_EMAIL]"));
    assert!(body.text.contains("[REDACTED_PHONE]"));
}

#[tokio::test]
async fn test_post_response_rejects_when_text_is_only_xss_script() {
    let app = setup_test_app();
    let company_slug = "company-corp-12";
    let owner_wallet = "0x20d";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: "<script>alert('only script');</script>".to_string(),
            company_slug: None,
        })
        .await;

    // After sanitization, text is empty, so it must return 400 Bad Request
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_response_404_when_no_response_exists() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let res = app.get(&format!("/posts/{}/response", review_id)).await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_response_200_when_response_exists() {
    let app = setup_test_app();
    let company_slug = "company-corp-13";
    let owner_wallet = "0x20e";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    let post_res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: "Official statement for verification.".to_string(),
            company_slug: None,
        })
        .await;
    assert_eq!(post_res.status_code(), StatusCode::OK);

    let get_res = app.get(&format!("/posts/{}/response", review_id)).await;
    assert_eq!(get_res.status_code(), StatusCode::OK);
    let body: CompanyResponse = get_res.json();
    assert_eq!(body.review_id, review_id);
    assert_eq!(body.text, "Official statement for verification.");
}

#[tokio::test]
async fn test_get_response_audit_404_when_no_response_exists() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let res = app
        .get(&format!("/posts/{}/response/audit", review_id))
        .await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_response_audit_empty_list_when_no_edits() {
    let app = setup_test_app();
    let company_slug = "company-corp-14";
    let owner_wallet = "0x20f";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);
    COMPANY_RESPONSE_SERVICE.register_review(review_id, company_slug);

    let token = create_auth_token(owner_wallet);

    let post_res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: "Original unedited response.".to_string(),
            company_slug: None,
        })
        .await;
    assert_eq!(post_res.status_code(), StatusCode::OK);

    let audit_res = app
        .get(&format!("/posts/{}/response/audit", review_id))
        .await;
    assert_eq!(audit_res.status_code(), StatusCode::OK);
    let audits: Vec<CompanyResponseAudit> = audit_res.json();
    assert!(audits.is_empty());
}

#[tokio::test]
async fn test_invalid_uuid_returns_400_bad_request() {
    let app = setup_test_app();
    let token = create_auth_token("0x210");

    let post_res = app
        .post("/posts/not-a-valid-uuid/response")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: "Some text".to_string(),
            company_slug: None,
        })
        .await;
    assert_eq!(post_res.status_code(), StatusCode::BAD_REQUEST);

    let get_res = app.get("/posts/not-a-valid-uuid/response").await;
    assert_eq!(get_res.status_code(), StatusCode::BAD_REQUEST);

    let audit_res = app.get("/posts/not-a-valid-uuid/response/audit").await;
    assert_eq!(audit_res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_post_response_with_company_slug_in_body() {
    let app = setup_test_app();
    let company_slug = "company-dynamic-slug";
    let owner_wallet = "0x211";
    let review_id = Uuid::new_v4();

    COMPANY_RESPONSE_SERVICE.add_company_owner(company_slug, owner_wallet);

    let token = create_auth_token(owner_wallet);

    // Review not pre-registered; company_slug provided in body
    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: "Providing company slug in payload.".to_string(),
            company_slug: Some(company_slug.to_string()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: CompanyResponse = res.json();
    assert_eq!(body.company_slug, company_slug);
}

#[tokio::test]
async fn test_post_response_unregistered_review_without_slug_returns_404() {
    let app = setup_test_app();
    let owner_wallet = "0x212";
    let review_id = Uuid::new_v4();

    let token = create_auth_token(owner_wallet);

    let res = app
        .post(&format!("/posts/{}/response", review_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateOrUpdateResponseDto {
            text: "Orphaned review without company slug.".to_string(),
            company_slug: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}
