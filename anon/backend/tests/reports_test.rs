use axum::http::{StatusCode, header};
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::models::report::{
    AdminReportsResponse, CreateReportRequest, CreateReportResponse, ReportReasonCode,
    ReportStatus, ResolveReportRequest, ReviewReport,
};
use backend::services::report_service::{REPORT_SERVICE, hash_reporter_id};
use uuid::Uuid;

fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-reports") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-reports") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

// =========================================================================
// SECTION 1: UNIT TESTS - PSEUDONYMIZATION & PRIVACY
// =========================================================================

#[test]
fn test_unit_reporter_id_pseudonymization() {
    let wallet = "0x0000000000000000000000000000000000000000000000000000000000000101";
    let hashed = hash_reporter_id(wallet);
    assert!(hashed.starts_with("rep_"));
    assert!(!hashed.contains(wallet));

    // Deterministic: same wallet generates same pseudonym
    assert_eq!(hashed, hash_reporter_id(wallet));
    assert_eq!(hashed, hash_reporter_id("0x101"));
}

// =========================================================================
// SECTION 2: INTEGRATION TESTS - POST /reports (USER FLAGGING)
// =========================================================================

#[tokio::test]
async fn test_post_report_success() {
    let app = setup_test_app();
    let reporter_wallet = "0x11";
    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let token = create_auth_token(reporter_wallet);
    let payload = CreateReportRequest {
        review_id,
        reason_code: ReportReasonCode::Spam,
        note: Some("Repetitive advertisement content".to_string()),
    };

    let res = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;

    assert_eq!(res.status_code(), StatusCode::CREATED);
    let body: CreateReportResponse = res.json();
    assert_eq!(body.status, ReportStatus::Open);

    // Verify stored report preserves reporter privacy
    let stored = REPORT_SERVICE.get_report(body.id).unwrap();
    assert_eq!(stored.review_id, review_id);
    assert_eq!(stored.reason_code, ReportReasonCode::Spam);
    assert_eq!(stored.reporter_user_id, hash_reporter_id(reporter_wallet));
    assert!(!stored.reporter_user_id.contains(reporter_wallet));
}

#[tokio::test]
async fn test_post_report_unauthorized_missing_jwt() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let res = app
        .post("/reports")
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Harassment,
            note: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_post_report_unauthorized_invalid_jwt() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();

    let res = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, "Bearer invalid-tampered-token")
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Harassment,
            note: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_post_report_review_not_found() {
    let app = setup_test_app();
    let token = create_auth_token("0x12");
    let unreg_id = Uuid::new_v4();

    let res = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReportRequest {
            review_id: unreg_id,
            reason_code: ReportReasonCode::PiiLeak,
            note: Some("Private address leaked".to_string()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
    let json: serde_json::Value = res.json();
    assert_eq!(json["error"], "review not found");
}

#[tokio::test]
async fn test_post_report_duplicate_prevention_open() {
    let app = setup_test_app();
    let reporter = "0x13";
    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let token = create_auth_token(reporter);
    let payload = CreateReportRequest {
        review_id,
        reason_code: ReportReasonCode::FalseInformation,
        note: Some("First report".to_string()),
    };

    // First report succeeds
    let res_1 = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;
    assert_eq!(res_1.status_code(), StatusCode::CREATED);

    // Second report from same reporter while Open fails with 409 Conflict
    let res_2 = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;
    assert_eq!(res_2.status_code(), StatusCode::CONFLICT);
    let json: serde_json::Value = res_2.json();
    assert_eq!(json["error"], "duplicate active report on this review");
}

#[tokio::test]
async fn test_post_report_duplicate_prevention_in_review() {
    let app = setup_test_app();
    let reporter = "0x14";
    let admin = "0x15";
    REPORT_SERVICE.add_admin(admin);

    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let token = create_auth_token(reporter);
    let payload = CreateReportRequest {
        review_id,
        reason_code: ReportReasonCode::HateSpeech,
        note: None,
    };

    let res_1 = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;
    let body_1: CreateReportResponse = res_1.json();

    // Admin sets status to InReview
    let _ = REPORT_SERVICE.resolve_report(
        body_1.id,
        admin,
        ResolveReportRequest {
            status: ReportStatus::InReview,
            resolution_note: Some("Investigating".to_string()),
        },
    );

    // Second report from same reporter still rejected as InReview
    let res_2 = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;
    assert_eq!(res_2.status_code(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_post_report_allowed_after_resolved() {
    let app = setup_test_app();
    let reporter = "0x16";
    let admin = "0x17";
    REPORT_SERVICE.add_admin(admin);

    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let token = create_auth_token(reporter);
    let payload = CreateReportRequest {
        review_id,
        reason_code: ReportReasonCode::Other,
        note: Some("Old complaint".to_string()),
    };

    let res_1 = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&payload)
        .await;
    let body_1: CreateReportResponse = res_1.json();

    // Admin resolves report to ResolvedDismissed
    let _ = REPORT_SERVICE.resolve_report(
        body_1.id,
        admin,
        ResolveReportRequest {
            status: ReportStatus::ResolvedDismissed,
            resolution_note: Some("No violation found".to_string()),
        },
    );

    // Now reporter can submit a new report
    let res_2 = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Other,
            note: Some("New complaint with more evidence".to_string()),
        })
        .await;
    assert_eq!(res_2.status_code(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_post_report_multiple_distinct_reporters_allowed() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let rep_1 = "0x18";
    let rep_2 = "0x19";

    let token_1 = create_auth_token(rep_1);
    let token_2 = create_auth_token(rep_2);

    let res_1 = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token_1))
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Spam,
            note: None,
        })
        .await;
    assert_eq!(res_1.status_code(), StatusCode::CREATED);

    let res_2 = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token_2))
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Spam,
            note: None,
        })
        .await;
    assert_eq!(res_2.status_code(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_post_report_note_too_long() {
    let app = setup_test_app();
    let token = create_auth_token("0x1a");
    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let res = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Other,
            note: Some("a".repeat(1001)),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let json: serde_json::Value = res.json();
    assert!(json["error"].as_str().unwrap().contains("1000"));
}

#[tokio::test]
async fn test_post_report_note_boundary_1000_chars() {
    let app = setup_test_app();
    let token = create_auth_token("0x1b");
    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let res = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Other,
            note: Some("x".repeat(1000)),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::CREATED);
}

#[tokio::test]
async fn test_post_report_xss_sanitization() {
    let app = setup_test_app();
    let token = create_auth_token("0x1c");
    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let res = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Harassment,
            note: Some("Toxic content: <script>alert('xss')</script> please review.".to_string()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::CREATED);
    let body: CreateReportResponse = res.json();

    let stored = REPORT_SERVICE.get_report(body.id).unwrap();
    let note = stored.note.unwrap();
    assert!(!note.contains("<script>"));
    assert!(!note.contains("alert"));
    assert!(note.contains("Toxic content:"));
}

#[tokio::test]
async fn test_all_reason_codes_accepted() {
    let app = setup_test_app();
    let reasons = [
        ReportReasonCode::Spam,
        ReportReasonCode::Harassment,
        ReportReasonCode::HateSpeech,
        ReportReasonCode::PiiLeak,
        ReportReasonCode::FalseInformation,
        ReportReasonCode::Other,
    ];

    for (i, reason) in reasons.iter().enumerate() {
        let review_id = Uuid::new_v4();
        REPORT_SERVICE.register_review(review_id);
        let wallet = format!("0x{:x}", 0x400 + i);
        let token = create_auth_token(&wallet);

        let res = app
            .post("/reports")
            .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
            .json(&CreateReportRequest {
                review_id,
                reason_code: *reason,
                note: None,
            })
            .await;

        assert_eq!(res.status_code(), StatusCode::CREATED);
    }
}

// =========================================================================
// SECTION 3: INTEGRATION TESTS - ADMIN QUEUE & REPORT RESOLUTION
// =========================================================================

#[tokio::test]
async fn test_get_admin_reports_forbidden_non_admin() {
    let app = setup_test_app();
    let token = create_auth_token("0x1d");

    let res = app
        .get("/admin/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_get_admin_reports_unauthorized() {
    let app = setup_test_app();
    let res = app.get("/admin/reports").await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_admin_reports_success_admin() {
    let app = setup_test_app();
    let admin = "0x1e";
    REPORT_SERVICE.add_admin(admin);

    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);
    let _ = REPORT_SERVICE.create_report(
        CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Spam,
            note: Some("Queue check".to_string()),
        },
        "0x1f",
    );

    let token = create_auth_token(admin);
    let res = app
        .get("/admin/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: AdminReportsResponse = res.json();
    assert!(!body.items.is_empty());
}

#[tokio::test]
async fn test_get_admin_reports_filter_status() {
    let app = setup_test_app();
    let admin = "0x20";
    REPORT_SERVICE.add_admin(admin);

    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    // Create report 1: left Open
    let _ = REPORT_SERVICE.create_report(
        CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::PiiLeak,
            note: None,
        },
        "0x21",
    );

    // Create report 2: resolved to ResolvedRemoved
    let rep_2 = REPORT_SERVICE
        .create_report(
            CreateReportRequest {
                review_id,
                reason_code: ReportReasonCode::HateSpeech,
                note: None,
            },
            "0x22",
        )
        .unwrap();

    let _ = REPORT_SERVICE.resolve_report(
        rep_2.id,
        admin,
        ResolveReportRequest {
            status: ReportStatus::ResolvedRemoved,
            resolution_note: Some("Confirmed hate speech".to_string()),
        },
    );

    let token = create_auth_token(admin);

    // Query open
    let res_open = app
        .get("/admin/reports?status=open")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res_open.status_code(), StatusCode::OK);
    let body_open: AdminReportsResponse = res_open.json();
    for item in &body_open.items {
        assert_eq!(item.status, ReportStatus::Open);
    }

    // Query resolved_removed
    let res_rem = app
        .get("/admin/reports?status=resolved_removed")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res_rem.status_code(), StatusCode::OK);
    let body_rem: AdminReportsResponse = res_rem.json();
    assert!(!body_rem.items.is_empty());
    for item in &body_rem.items {
        assert_eq!(item.status, ReportStatus::ResolvedRemoved);
    }
}

#[tokio::test]
async fn test_put_admin_reports_resolve_to_in_review() {
    let app = setup_test_app();
    let admin = "0x23";
    REPORT_SERVICE.add_admin(admin);

    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let created = REPORT_SERVICE
        .create_report(
            CreateReportRequest {
                review_id,
                reason_code: ReportReasonCode::FalseInformation,
                note: Some("Fact check required".to_string()),
            },
            "0x24",
        )
        .unwrap();

    let token = create_auth_token(admin);
    let res = app
        .put(&format!("/admin/reports/{}", created.id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&ResolveReportRequest {
            status: ReportStatus::InReview,
            resolution_note: Some("Assigned to senior auditor".to_string()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: ReviewReport = res.json();
    assert_eq!(body.id, created.id);
    assert_eq!(body.status, ReportStatus::InReview);
    assert_eq!(
        body.resolution_note,
        Some("Assigned to senior auditor".to_string())
    );
    assert_eq!(body.resolved_by, Some(backend::services::report_service::canonical_wallet(admin)));
}

#[tokio::test]
async fn test_put_admin_reports_resolve_to_removed_and_tracks_removal() {
    let app = setup_test_app();
    let admin = "0x25";
    REPORT_SERVICE.add_admin(admin);

    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);
    assert!(!REPORT_SERVICE.is_review_removed(review_id));

    let created = REPORT_SERVICE
        .create_report(
            CreateReportRequest {
                review_id,
                reason_code: ReportReasonCode::Harassment,
                note: Some("Severe harassment".to_string()),
            },
            "0x26",
        )
        .unwrap();

    let token = create_auth_token(admin);
    let res = app
        .put(&format!("/admin/reports/{}", created.id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&ResolveReportRequest {
            status: ReportStatus::ResolvedRemoved,
            resolution_note: Some("Review removed for violation".to_string()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: ReviewReport = res.json();
    assert_eq!(body.status, ReportStatus::ResolvedRemoved);

    // Verify review is tracked as removed
    assert!(REPORT_SERVICE.is_review_removed(review_id));
}

#[tokio::test]
async fn test_put_admin_reports_resolve_to_dismissed() {
    let app = setup_test_app();
    let admin = "0x27";
    REPORT_SERVICE.add_admin(admin);

    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    let created = REPORT_SERVICE
        .create_report(
            CreateReportRequest {
                review_id,
                reason_code: ReportReasonCode::Spam,
                note: Some("Not spam".to_string()),
            },
            "0x28",
        )
        .unwrap();

    let token = create_auth_token(admin);
    let res = app
        .put(&format!("/admin/reports/{}", created.id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&ResolveReportRequest {
            status: ReportStatus::ResolvedDismissed,
            resolution_note: Some("Content does not violate spam guidelines".to_string()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: ReviewReport = res.json();
    assert_eq!(body.status, ReportStatus::ResolvedDismissed);
    assert!(!REPORT_SERVICE.is_review_removed(review_id));
}

#[tokio::test]
async fn test_put_admin_reports_404_nonexistent() {
    let app = setup_test_app();
    let admin = "0x29";
    REPORT_SERVICE.add_admin(admin);
    let token = create_auth_token(admin);
    let unreg_id = Uuid::new_v4();

    let res = app
        .put(&format!("/admin/reports/{}", unreg_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&ResolveReportRequest {
            status: ReportStatus::ResolvedDismissed,
            resolution_note: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_put_admin_reports_forbidden_non_admin() {
    let app = setup_test_app();
    let non_admin = "0x2a";
    let token = create_auth_token(non_admin);
    let some_id = Uuid::new_v4();

    let res = app
        .put(&format!("/admin/reports/{}", some_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&ResolveReportRequest {
            status: ReportStatus::InReview,
            resolution_note: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_invalid_uuid_returns_400_bad_request() {
    let app = setup_test_app();
    let admin = "0x2b";
    REPORT_SERVICE.add_admin(admin);
    let token = create_auth_token(admin);

    let res = app
        .put("/admin/reports/not-a-valid-uuid")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&ResolveReportRequest {
            status: ReportStatus::InReview,
            resolution_note: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_post_report_without_note_field() {
    let app = setup_test_app();
    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);
    let token = create_auth_token("0x2c");

    let res = app
        .post("/reports")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReportRequest {
            review_id,
            reason_code: ReportReasonCode::Spam,
            note: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::CREATED);
    let body: CreateReportResponse = res.json();
    let stored = REPORT_SERVICE.get_report(body.id).unwrap();
    assert_eq!(stored.note, None);
}

#[tokio::test]
async fn test_admin_reports_pagination_limit_and_cursor() {
    let app = setup_test_app();
    let admin = "0x2d";
    REPORT_SERVICE.add_admin(admin);

    let review_id = Uuid::new_v4();
    REPORT_SERVICE.register_review(review_id);

    for i in 0..5 {
        let _ = REPORT_SERVICE.create_report(
            CreateReportRequest {
                review_id,
                reason_code: ReportReasonCode::Spam,
                note: Some(format!("Report note {}", i)),
            },
            &format!("0x{:x}", 0x500 + i),
        );
    }

    let token = create_auth_token(admin);
    let page1: AdminReportsResponse = app
        .get("/admin/reports?limit=2")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await
        .json();

    assert_eq!(page1.items.len(), 2);
    assert!(page1.cursor.is_some());

    let cursor = page1.cursor.unwrap();
    let page2: AdminReportsResponse = app
        .get(&format!("/admin/reports?limit=2&cursor={}", cursor))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await
        .json();

    assert_eq!(page2.items.len(), 2);
    assert_ne!(page1.items[0].id, page2.items[0].id);
}

#[test]
fn test_unit_report_status_default_is_open() {
    let status = ReportStatus::default();
    assert_eq!(status, ReportStatus::Open);
}

