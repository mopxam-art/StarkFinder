use axum::http::StatusCode;
use axum_test::TestServer;
use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use backend::libs::db::AppState;
use backend::models::draft::{
    AuditHistoryResponse, CreateDraftDto, DeleteDraftResponse, DraftResponse,
    EditPublishedReviewDto, EditPublishedReviewResponse, PublishResponse, PublishedReview,
    ReviewDraft, UpdateDraftDto,
};
use backend::services::draft_service::DRAFT_SERVICE;

/// Helper to spin up an ephemeral test server instance
fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-drafts") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

/// Helper to generate valid Starknet wallet auth JWT
fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-drafts") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

// ============================================================================
// GROUP 1: DRAFT CREATION & VALIDATION (Tests 01-04)
// ============================================================================

#[tokio::test]
async fn test_01_create_draft_success() {
    let server = setup_test_app();
    let wallet = "0x01";
    let token = create_auth_token(wallet);

    let dto = CreateDraftDto {
        company_slug: "shonen-labs".into(),
        title: "Initial Impressions on StarkFinder".into(),
        content: "Great developer tooling for Starknet ecosystem.".into(),
        rating: 5,
        tags: vec!["tooling".into(), "starknet".into()],
    };

    let res = server
        .post("/posts/drafts")
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::CREATED);
    let body: DraftResponse = res.json();
    assert_eq!(body.draft.title, "Initial Impressions on StarkFinder");
    assert_eq!(body.draft.rating, 5);
    assert_eq!(body.draft.tags.len(), 2);
    assert!(body.message.contains("successfully"));
}

#[tokio::test]
async fn test_02_create_draft_empty_title_fails() {
    let server = setup_test_app();
    let wallet = "0x02";
    let token = create_auth_token(wallet);

    let dto = CreateDraftDto {
        company_slug: "argent".into(),
        title: "   ".into(),
        content: "Valid content here.".into(),
        rating: 4,
        tags: vec![],
    };

    let res = server
        .post("/posts/drafts")
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_03_create_draft_empty_content_fails() {
    let server = setup_test_app();
    let wallet = "0x03";
    let token = create_auth_token(wallet);

    let dto = CreateDraftDto {
        company_slug: "braavos".into(),
        title: "Valid Title".into(),
        content: "".into(),
        rating: 4,
        tags: vec![],
    };

    let res = server
        .post("/posts/drafts")
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_04_create_draft_invalid_rating_fails() {
    let server = setup_test_app();
    let wallet = "0x04";
    let token = create_auth_token(wallet);

    // Rating 0 is invalid
    let dto = CreateDraftDto {
        company_slug: "avnu".into(),
        title: "Rating test".into(),
        content: "Testing invalid rating.".into(),
        rating: 0,
        tags: vec![],
    };

    let res = server
        .post("/posts/drafts")
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// GROUP 2: DRAFT UPDATE & ACCESS CONTROL (Tests 05-07)
// ============================================================================

#[tokio::test]
async fn test_05_update_draft_by_author_success() {
    let server = setup_test_app();
    let wallet = "0x05";
    let token = create_auth_token(wallet);

    let draft = DRAFT_SERVICE
        .create_draft(
            wallet,
            CreateDraftDto {
                company_slug: "ekubo".into(),
                title: "Draft 05".into(),
                content: "Original draft content".into(),
                rating: 3,
                tags: vec!["amm".into()],
            },
        )
        .unwrap();

    let update_dto = UpdateDraftDto {
        company_slug: None,
        title: Some("Updated Title 05".into()),
        content: Some("Revised draft content with more details".into()),
        rating: Some(4),
        tags: Some(vec!["amm".into(), "v3".into()]),
    };

    let res = server
        .put(&format!("/posts/drafts/{}", draft.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&update_dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: DraftResponse = res.json();
    assert_eq!(body.draft.title, "Updated Title 05");
    assert_eq!(body.draft.content, "Revised draft content with more details");
    assert_eq!(body.draft.rating, 4);
    assert_eq!(body.draft.tags.len(), 2);
}

#[tokio::test]
async fn test_06_update_draft_by_non_author_forbidden() {
    let server = setup_test_app();
    let owner_wallet = "0x06a";
    let intruder_wallet = "0x06b";

    let draft = DRAFT_SERVICE
        .create_draft(
            owner_wallet,
            CreateDraftDto {
                company_slug: "argent".into(),
                title: "Owner Draft".into(),
                content: "Secret unreleased draft".into(),
                rating: 5,
                tags: vec![],
            },
        )
        .unwrap();

    let intruder_token = create_auth_token(intruder_wallet);
    let update_dto = UpdateDraftDto {
        title: Some("Hacked Title".into()),
        ..Default::default()
    };

    let res = server
        .put(&format!("/posts/drafts/{}", draft.id))
        .add_header("Authorization", format!("Bearer {}", intruder_token))
        .json(&update_dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_07_update_nonexistent_draft_returns_404() {
    let server = setup_test_app();
    let wallet = "0x07";
    let token = create_auth_token(wallet);
    let random_id = Uuid::new_v4();

    let update_dto = UpdateDraftDto {
        title: Some("Any title".into()),
        ..Default::default()
    };

    let res = server
        .put(&format!("/posts/drafts/{}", random_id))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&update_dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

// ============================================================================
// GROUP 3: DRAFT DELETION & LISTING (Tests 08-11)
// ============================================================================

#[tokio::test]
async fn test_08_delete_draft_by_author_success() {
    let server = setup_test_app();
    let wallet = "0x08";
    let token = create_auth_token(wallet);

    let draft = DRAFT_SERVICE
        .create_draft(
            wallet,
            CreateDraftDto {
                company_slug: "starknet-id".into(),
                title: "To be deleted".into(),
                content: "Content".into(),
                rating: 3,
                tags: vec![],
            },
        )
        .unwrap();

    let del_res = server
        .delete(&format!("/posts/drafts/{}", draft.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(del_res.status_code(), StatusCode::OK);
    let body: DeleteDraftResponse = del_res.json();
    assert_eq!(body.id, draft.id);

    // Verify subsequent lookup returns 404
    let get_res = server
        .get(&format!("/posts/drafts/{}", draft.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    assert_eq!(get_res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_09_delete_draft_by_non_author_forbidden() {
    let server = setup_test_app();
    let owner_wallet = "0x09a";
    let intruder_wallet = "0x09b";

    let draft = DRAFT_SERVICE
        .create_draft(
            owner_wallet,
            CreateDraftDto {
                company_slug: "nostra".into(),
                title: "Private Draft".into(),
                content: "Important content".into(),
                rating: 4,
                tags: vec![],
            },
        )
        .unwrap();

    let intruder_token = create_auth_token(intruder_wallet);
    let res = server
        .delete(&format!("/posts/drafts/{}", draft.id))
        .add_header("Authorization", format!("Bearer {}", intruder_token))
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_10_delete_nonexistent_draft_returns_404() {
    let server = setup_test_app();
    let wallet = "0x10";
    let token = create_auth_token(wallet);
    let random_id = Uuid::new_v4();

    let res = server
        .delete(&format!("/posts/drafts/{}", random_id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_11_list_user_drafts_isolation() {
    let server = setup_test_app();
    let user_a = "0x11a";
    let user_b = "0x11b";

    // User A creates 2 drafts
    for i in 1..=2 {
        DRAFT_SERVICE
            .create_draft(
                user_a,
                CreateDraftDto {
                    company_slug: "shonen-labs".into(),
                    title: format!("User A Draft {}", i),
                    content: "Draft content".into(),
                    rating: 5,
                    tags: vec![],
                },
            )
            .unwrap();
    }

    // User B creates 1 draft
    DRAFT_SERVICE
        .create_draft(
            user_b,
            CreateDraftDto {
                company_slug: "shonen-labs".into(),
                title: "User B Draft 1".into(),
                content: "User B content".into(),
                rating: 4,
                tags: vec![],
            },
        )
        .unwrap();

    let token_a = create_auth_token(user_a);
    let res = server
        .get("/posts/drafts")
        .add_header("Authorization", format!("Bearer {}", token_a))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let drafts: Vec<ReviewDraft> = res.json();
    assert_eq!(drafts.len(), 2);
    for d in drafts {
        assert!(d.title.contains("User A"));
    }
}

// ============================================================================
// GROUP 4: DRAFT PUBLISHING (Tests 12-15)
// ============================================================================

#[tokio::test]
async fn test_12_publish_draft_success() {
    let server = setup_test_app();
    let wallet = "0x12";
    let token = create_auth_token(wallet);

    let draft = DRAFT_SERVICE
        .create_draft(
            wallet,
            CreateDraftDto {
                company_slug: "fibrous".into(),
                title: "DEX Aggregator Deep Dive".into(),
                content: "Comprehensive routing analysis across Starknet DEXes.".into(),
                rating: 5,
                tags: vec!["dex".into(), "aggregator".into()],
            },
        )
        .unwrap();

    let res = server
        .post(&format!("/posts/drafts/{}/publish", draft.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: PublishResponse = res.json();
    assert_eq!(body.published_review.id, draft.id);
    assert_eq!(body.published_review.title, draft.title);
    assert!(!body.published_review.is_edited);
    assert!(body.message.contains("published successfully"));
}

#[tokio::test]
async fn test_13_published_draft_removed_from_drafts() {
    let server = setup_test_app();
    let wallet = "0x13";
    let token = create_auth_token(wallet);

    let draft = DRAFT_SERVICE
        .create_draft(
            wallet,
            CreateDraftDto {
                company_slug: "argent".into(),
                title: "Wallet Review 13".into(),
                content: "Content".into(),
                rating: 4,
                tags: vec![],
            },
        )
        .unwrap();

    // Publish
    server
        .post(&format!("/posts/drafts/{}/publish", draft.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    // Must be gone from drafts
    let get_draft_res = server
        .get(&format!("/posts/drafts/{}", draft.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    assert_eq!(get_draft_res.status_code(), StatusCode::NOT_FOUND);

    // Must be available as published review
    let get_pub_res = server.get(&format!("/posts/{}", draft.id)).await;
    assert_eq!(get_pub_res.status_code(), StatusCode::OK);
    let pub_rev: PublishedReview = get_pub_res.json();
    assert_eq!(pub_rev.id, draft.id);
}

#[tokio::test]
async fn test_14_publish_draft_by_non_author_forbidden() {
    let server = setup_test_app();
    let owner_wallet = "0x14a";
    let intruder_wallet = "0x14b";

    let draft = DRAFT_SERVICE
        .create_draft(
            owner_wallet,
            CreateDraftDto {
                company_slug: "braavos".into(),
                title: "Owner Title".into(),
                content: "Content".into(),
                rating: 5,
                tags: vec![],
            },
        )
        .unwrap();

    let intruder_token = create_auth_token(intruder_wallet);
    let res = server
        .post(&format!("/posts/drafts/{}/publish", draft.id))
        .add_header("Authorization", format!("Bearer {}", intruder_token))
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_15_publish_nonexistent_draft_returns_404() {
    let server = setup_test_app();
    let wallet = "0x15";
    let token = create_auth_token(wallet);
    let random_id = Uuid::new_v4();

    let res = server
        .post(&format!("/posts/drafts/{}/publish", random_id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

// ============================================================================
// GROUP 5: TIME-WINDOWED EDITS & AUDIT LOG (Tests 16-22)
// ============================================================================

#[tokio::test]
async fn test_16_edit_published_review_within_window_success() {
    let server = setup_test_app();
    let wallet = "0x16";
    let token = create_auth_token(wallet);

    let draft = DRAFT_SERVICE
        .create_draft(
            wallet,
            CreateDraftDto {
                company_slug: "zklend".into(),
                title: "Lending Market Review".into(),
                content: "Original review text with a typo.".into(),
                rating: 4,
                tags: vec![],
            },
        )
        .unwrap();

    let published = DRAFT_SERVICE.publish_draft(wallet, draft.id).unwrap();

    // Edit within 15 minutes window
    let edit_dto = EditPublishedReviewDto {
        content: "Revised review text with typo fixed and updated rates.".into(),
        edit_reason: Some("Fixing minor typos".into()),
    };

    let res = server
        .patch(&format!("/posts/{}", published.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&edit_dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: EditPublishedReviewResponse = res.json();
    assert_eq!(body.published_review.content, edit_dto.content);
    assert!(body.published_review.is_edited);
    assert_eq!(
        body.audit_entry.previous_content,
        "Original review text with a typo."
    );
    assert_eq!(body.audit_entry.new_content, edit_dto.content);
    assert_eq!(body.audit_entry.edit_reason, Some("Fixing minor typos".into()));
}

#[tokio::test]
async fn test_17_edit_published_review_after_window_expired_fails() {
    let server = setup_test_app();
    let wallet = "0x17";
    let token = create_auth_token(wallet);

    let draft = DRAFT_SERVICE
        .create_draft(
            wallet,
            CreateDraftDto {
                company_slug: "carmine".into(),
                title: "Options Protocol".into(),
                content: "Original text".into(),
                rating: 5,
                tags: vec![],
            },
        )
        .unwrap();

    let published = DRAFT_SERVICE.publish_draft(wallet, draft.id).unwrap();

    // Simulate review published 16 minutes ago (960s > 900s limit)
    let expired_published_at = Utc::now() - Duration::seconds(960);
    DRAFT_SERVICE
        .set_review_published_at(published.id, expired_published_at)
        .unwrap();

    let edit_dto = EditPublishedReviewDto {
        content: "Late edit attempting to bypass window".into(),
        edit_reason: None,
    };

    let res = server
        .patch(&format!("/posts/{}", published.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&edit_dto)
        .await;

    // Must return 400 Bad Request: edit window expired
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let err: serde_json::Value = res.json();
    assert!(
        err["error"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("edit window expired")
    );
}

#[tokio::test]
async fn test_18_edit_published_review_by_non_author_forbidden() {
    let server = setup_test_app();
    let owner = "0x18a";
    let intruder = "0x18b";

    let draft = DRAFT_SERVICE
        .create_draft(
            owner,
            CreateDraftDto {
                company_slug: "ekubo".into(),
                title: "Ekubo Review".into(),
                content: "Original text".into(),
                rating: 5,
                tags: vec![],
            },
        )
        .unwrap();

    let published = DRAFT_SERVICE.publish_draft(owner, draft.id).unwrap();

    let intruder_token = create_auth_token(intruder);
    let edit_dto = EditPublishedReviewDto {
        content: "Malicious edit by someone else".into(),
        edit_reason: None,
    };

    let res = server
        .patch(&format!("/posts/{}", published.id))
        .add_header("Authorization", format!("Bearer {}", intruder_token))
        .json(&edit_dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_19_edit_nonexistent_review_returns_404() {
    let server = setup_test_app();
    let wallet = "0x19";
    let token = create_auth_token(wallet);
    let random_id = Uuid::new_v4();

    let edit_dto = EditPublishedReviewDto {
        content: "Text".into(),
        edit_reason: None,
    };

    let res = server
        .patch(&format!("/posts/{}", random_id))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&edit_dto)
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_20_audit_log_records_multiple_edits() {
    let server = setup_test_app();
    let wallet = "0x20";
    let token = create_auth_token(wallet);

    let draft = DRAFT_SERVICE
        .create_draft(
            wallet,
            CreateDraftDto {
                company_slug: "shonen-labs".into(),
                title: "Evolving Review".into(),
                content: "v1: Original text".into(),
                rating: 5,
                tags: vec![],
            },
        )
        .unwrap();

    let published = DRAFT_SERVICE.publish_draft(wallet, draft.id).unwrap();

    // Edit 1
    server
        .patch(&format!("/posts/{}", published.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&EditPublishedReviewDto {
            content: "v2: Second iteration".into(),
            edit_reason: Some("First revision".into()),
        })
        .await;

    // Edit 2
    server
        .patch(&format!("/posts/{}", published.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&EditPublishedReviewDto {
            content: "v3: Third iteration".into(),
            edit_reason: Some("Second revision".into()),
        })
        .await;

    let audit_history = DRAFT_SERVICE.get_review_audit_history(published.id);
    assert_eq!(audit_history.len(), 2);
    assert_eq!(audit_history[0].previous_content, "v1: Original text");
    assert_eq!(audit_history[0].new_content, "v2: Second iteration");
    assert_eq!(audit_history[1].previous_content, "v2: Second iteration");
    assert_eq!(audit_history[1].new_content, "v3: Third iteration");
}

#[tokio::test]
async fn test_21_get_audit_history_endpoint() {
    let server = setup_test_app();
    let wallet = "0x21";
    let token = create_auth_token(wallet);

    let draft = DRAFT_SERVICE
        .create_draft(
            wallet,
            CreateDraftDto {
                company_slug: "argent".into(),
                title: "Audit Test".into(),
                content: "Initial content".into(),
                rating: 4,
                tags: vec![],
            },
        )
        .unwrap();

    let published = DRAFT_SERVICE.publish_draft(wallet, draft.id).unwrap();

    server
        .patch(&format!("/posts/{}", published.id))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&EditPublishedReviewDto {
            content: "Updated content".into(),
            edit_reason: Some("Update".into()),
        })
        .await;

    let res = server.get(&format!("/posts/{}/audit", published.id)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: AuditHistoryResponse = res.json();
    assert_eq!(body.review_id, published.id);
    assert_eq!(body.entries.len(), 1);
}

#[tokio::test]
async fn test_22_unauthorized_endpoints_rejected() {
    let server = setup_test_app();

    // POST /posts/drafts without token -> 401
    let res1 = server
        .post("/posts/drafts")
        .json(&json!({
            "company_slug": "argent",
            "title": "T",
            "content": "C",
            "rating": 5
        }))
        .await;
    assert_eq!(res1.status_code(), StatusCode::UNAUTHORIZED);

    // GET /posts/drafts without token -> 401
    let res2 = server.get("/posts/drafts").await;
    assert_eq!(res2.status_code(), StatusCode::UNAUTHORIZED);

    // PATCH /posts/{id} without token -> 401
    let res3 = server
        .patch(&format!("/posts/{}", Uuid::new_v4()))
        .json(&json!({ "content": "Updated" }))
        .await;
    assert_eq!(res3.status_code(), StatusCode::UNAUTHORIZED);
}
