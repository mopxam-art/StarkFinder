use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::json;
use uuid::Uuid;

use backend::libs::db::AppState;
use backend::models::notification::{
    DispatchNotificationDto, DispatchResponse, FollowKind, FollowResponse, FollowStatusResponse,
    MarkAllReadResponse, MarkReadResponse, NotificationType, NotificationsListRes,
    UnfollowResponse,
};
use backend::services::notification_service::{NOTIFICATION_STORE, canonical_user};

/// Helper to spin up an ephemeral test server instance
fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-notifications") };

    // Lazy unreachable pool so no actual external Postgres DB is required
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
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-notifications") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

// ============================================================================
// GROUP 1: FOLLOWS & SUBSCRIPTIONS LIFECYCLE (Tests 01-08)
// ============================================================================

#[tokio::test]
async fn test_01_follow_company_success() {
    let server = setup_test_app();
    let wallet = "0x01";
    let token = create_auth_token(wallet);

    let res = server
        .post("/companies/shonen-labs-01/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: FollowResponse = res.json();
    assert_eq!(body.kind, FollowKind::Company);
    assert_eq!(body.target_id, "shonen-labs-01");
    assert!(body.message.contains("Successfully followed"));
}

#[tokio::test]
async fn test_02_follow_tag_success() {
    let server = setup_test_app();
    let wallet = "0x02";
    let token = create_auth_token(wallet);

    let res = server
        .post("/tags/defi-02/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: FollowResponse = res.json();
    assert_eq!(body.kind, FollowKind::Tag);
    assert_eq!(body.target_id, "defi-02");
}

#[tokio::test]
async fn test_03_follow_idempotency_duplicate_protection() {
    let server = setup_test_app();
    let wallet = "0x03";
    let token = create_auth_token(wallet);

    // First follow
    let res1 = server
        .post("/companies/argent-03/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    assert_eq!(res1.status_code(), StatusCode::OK);

    // Second follow (idempotent duplicate call)
    let res2 = server
        .post("/companies/argent-03/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    assert_eq!(res2.status_code(), StatusCode::OK);

    // Followers list should still only contain 1 entry
    let followers = NOTIFICATION_STORE.list_followers(FollowKind::Company, "argent-03");
    assert_eq!(followers.len(), 1);
}

#[tokio::test]
async fn test_04_get_follow_status_true_and_false() {
    let server = setup_test_app();
    let wallet = "0x04";
    let token = create_auth_token(wallet);

    // Before following -> false
    let res1 = server
        .get("/companies/braavos-04/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    assert_eq!(res1.status_code(), StatusCode::OK);
    let body1: FollowStatusResponse = res1.json();
    assert!(!body1.is_following);

    // Follow braavos-04
    server
        .post("/companies/braavos-04/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    // After following -> true
    let res2 = server
        .get("/companies/braavos-04/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    assert_eq!(res2.status_code(), StatusCode::OK);
    let body2: FollowStatusResponse = res2.json();
    assert!(body2.is_following);
}

#[tokio::test]
async fn test_05_unfollow_company_success() {
    let server = setup_test_app();
    let wallet = "0x05";
    let token = create_auth_token(wallet);

    // Follow first
    server
        .post("/companies/avnu-05/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    // Unfollow
    let res = server
        .delete("/companies/avnu-05/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: UnfollowResponse = res.json();
    assert_eq!(body.kind, FollowKind::Company);
    assert_eq!(body.target_id, "avnu-05");

    // Status check should be false
    assert!(!NOTIFICATION_STORE.is_following(wallet, FollowKind::Company, "avnu-05"));
}

#[tokio::test]
async fn test_06_unfollow_nonexistent_returns_404() {
    let server = setup_test_app();
    let wallet = "0x06";
    let token = create_auth_token(wallet);

    // Attempt to unfollow something never followed
    let res = server
        .delete("/companies/nonexistent-org-06/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_07_invalid_slug_or_tag_empty() {
    let server = setup_test_app();
    let wallet = "0x07";
    let token = create_auth_token(wallet);

    // Space-only target ID should return 400 Bad Request
    let res = server
        .post("/tags/%20%20/follow")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_08_list_followers_ordering_and_deduplication() {
    let target = "starknet-id-08";

    // 3 users follow
    NOTIFICATION_STORE
        .follow_target("0x08a", FollowKind::Company, target)
        .unwrap();
    NOTIFICATION_STORE
        .follow_target("0x08b", FollowKind::Company, target)
        .unwrap();
    NOTIFICATION_STORE
        .follow_target("0x08c", FollowKind::Company, target)
        .unwrap();

    let followers = NOTIFICATION_STORE.list_followers(FollowKind::Company, target);
    assert_eq!(followers.len(), 3);
    // Followers should be sorted deterministically
    assert_eq!(followers[0], canonical_user("0x08a"));
    assert_eq!(followers[1], canonical_user("0x08b"));
    assert_eq!(followers[2], canonical_user("0x08c"));
}

// ============================================================================
// GROUP 2: NOTIFICATION DISPATCH & IN-APP ALERTS (Tests 09-15)
// ============================================================================

#[tokio::test]
async fn test_09_dispatch_single_notification() {
    let server = setup_test_app();

    let dto = DispatchNotificationDto {
        target_users: vec!["0x09".to_string()],
        notification_type: NotificationType::NewReview,
        payload: json!({ "review_id": "rev-101", "rating": 5 }),
    };

    let res = server.post("/notifications/dispatch").json(&dto).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: DispatchResponse = res.json();
    assert_eq!(body.dispatched_count, 1);
}

#[tokio::test]
async fn test_10_dispatch_multiple_recipients() {
    let server = setup_test_app();

    let dto = DispatchNotificationDto {
        target_users: vec![
            "0x10a".to_string(),
            "0x10b".to_string(),
            "0x10c".to_string(),
            "0x10d".to_string(),
            "0x10e".to_string(),
        ],
        notification_type: NotificationType::StatusChange,
        payload: json!({ "contract_address": "0xabc", "new_status": "audited" }),
    };

    let res = server.post("/notifications/dispatch").json(&dto).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: DispatchResponse = res.json();
    assert_eq!(body.dispatched_count, 5);

    // Each recipient should have exactly 1 notification
    for w in &dto.target_users {
        let notifs = NOTIFICATION_STORE.get_user_notifications(w, None, None, false);
        assert_eq!(notifs.total_count, 1);
    }
}

#[tokio::test]
async fn test_11_dispatch_deduplicates_duplicate_target_users() {
    let server = setup_test_app();

    let dto = DispatchNotificationDto {
        target_users: vec![
            "0x11a".to_string(),
            "0x11a".to_string(), // duplicate
            "0x11a".to_string(), // duplicate
            "0x11b".to_string(),
        ],
        notification_type: NotificationType::NewSummary,
        payload: json!({ "summary_title": "Monthly Starknet Round-Up" }),
    };

    let res = server.post("/notifications/dispatch").json(&dto).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body: DispatchResponse = res.json();
    assert_eq!(body.dispatched_count, 2);

    let notifs_11a = NOTIFICATION_STORE.get_user_notifications("0x11a", None, None, false);
    assert_eq!(notifs_11a.total_count, 1);
}

#[tokio::test]
async fn test_12_dispatch_different_notification_types() {
    let user = "0x12a";

    let types = [
        NotificationType::NewReview,
        NotificationType::NewSummary,
        NotificationType::StatusChange,
        NotificationType::Mention,
    ];

    for t in types {
        let count = NOTIFICATION_STORE
            .dispatch_notification(&[user.to_string()], t, json!({ "type": t.to_string() }))
            .unwrap();
        assert_eq!(count, 1);
    }

    let notifs = NOTIFICATION_STORE.get_user_notifications(user, None, None, false);
    assert_eq!(notifs.total_count, 4);
    assert_eq!(notifs.unread_count, 4);
}

#[tokio::test]
async fn test_13_subscriber_notification_pipeline() {
    let company = "ekubo-13";

    // 2 users follow Ekubo
    NOTIFICATION_STORE
        .follow_target("0x13a", FollowKind::Company, company)
        .unwrap();
    NOTIFICATION_STORE
        .follow_target("0x13b", FollowKind::Company, company)
        .unwrap();

    // Query subscribers and notify
    let followers = NOTIFICATION_STORE.list_followers(FollowKind::Company, company);
    assert_eq!(followers.len(), 2);

    let dispatched = NOTIFICATION_STORE
        .dispatch_notification(
            &followers,
            NotificationType::NewReview,
            json!({ "company": company, "review_score": 4.8 }),
        )
        .unwrap();

    assert_eq!(dispatched, 2);

    let u13a_notifs = NOTIFICATION_STORE.get_user_notifications("0x13a", None, None, false);
    assert_eq!(u13a_notifs.total_count, 1);
    assert_eq!(
        u13a_notifs.items[0].payload["company"],
        json!(company)
    );
}

#[tokio::test]
async fn test_14_empty_dispatch_targets() {
    let dispatched = NOTIFICATION_STORE
        .dispatch_notification(&[], NotificationType::Mention, json!({}))
        .unwrap();
    assert_eq!(dispatched, 0);
}

#[tokio::test]
async fn test_15_dispatch_preserves_complex_json_payload() {
    let user = "0x15a";
    let complex_payload = json!({
        "metadata": {
            "tags": ["defi", "amm", "starknet"],
            "score": 99.5,
            "verified": true,
            "nested": {
                "level2": {
                    "field": "deep"
                }
            }
        }
    });

    NOTIFICATION_STORE
        .dispatch_notification(
            &[user.to_string()],
            NotificationType::NewSummary,
            complex_payload.clone(),
        )
        .unwrap();

    let notifs = NOTIFICATION_STORE.get_user_notifications(user, None, None, false);
    assert_eq!(notifs.items[0].payload, complex_payload);
}

// ============================================================================
// GROUP 3: PRIVACY & DATA ISOLATION (Tests 16-19)
// ============================================================================

#[tokio::test]
async fn test_16_privacy_user_cannot_see_other_users_notifications() {
    let server = setup_test_app();
    let user_a = "0x16a";
    let user_b = "0x16b";

    // Dispatch 2 to User A, 1 to User B
    NOTIFICATION_STORE
        .dispatch_notification(
            &[user_a.to_string()],
            NotificationType::NewReview,
            json!({ "for": "User A" }),
        )
        .unwrap();
    NOTIFICATION_STORE
        .dispatch_notification(
            &[user_a.to_string()],
            NotificationType::Mention,
            json!({ "for": "User A" }),
        )
        .unwrap();
    NOTIFICATION_STORE
        .dispatch_notification(
            &[user_b.to_string()],
            NotificationType::StatusChange,
            json!({ "for": "User B" }),
        )
        .unwrap();

    let token_a = create_auth_token(user_a);
    let res = server
        .get("/me/notifications")
        .add_header("Authorization", format!("Bearer {}", token_a))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let list_a: NotificationsListRes = res.json();
    assert_eq!(list_a.total_count, 2);
    assert_eq!(list_a.items.len(), 2);
    // User A should not see User B's alert
    for item in list_a.items {
        assert_eq!(item.payload["for"], json!("User A"));
    }
}

#[tokio::test]
async fn test_17_privacy_user_cannot_mark_other_users_notification_as_read() {
    let server = setup_test_app();
    let user_owner = "0x17a";
    let user_intruder = "0x17b";

    // Create notification for owner
    NOTIFICATION_STORE
        .dispatch_notification(
            &[user_owner.to_string()],
            NotificationType::NewReview,
            json!({ "secret": "owner-data" }),
        )
        .unwrap();

    let owner_notifs =
        NOTIFICATION_STORE.get_user_notifications(user_owner, None, None, false);
    let target_id = owner_notifs.items[0].id;

    // Intruder attempts to mark owner's notification as read
    let token_intruder = create_auth_token(user_intruder);
    let res = server
        .patch(&format!("/me/notifications/{}", target_id))
        .add_header("Authorization", format!("Bearer {}", token_intruder))
        .await;

    // Must be rejected with 403 Forbidden
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);

    // Verify owner's notification is still unread
    let check = NOTIFICATION_STORE.get_user_notifications(user_owner, None, None, false);
    assert!(check.items[0].read_at.is_none());
}

#[tokio::test]
async fn test_18_privacy_mark_all_read_affects_only_calling_user() {
    let server = setup_test_app();
    let user_a = "0x18a";
    let user_b = "0x18b";

    NOTIFICATION_STORE
        .dispatch_notification(&[user_a.to_string(), user_b.to_string()], NotificationType::Mention, json!({}))
        .unwrap();

    let token_a = create_auth_token(user_a);
    let res = server
        .post("/me/notifications/read-all")
        .add_header("Authorization", format!("Bearer {}", token_a))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);

    // User A's alerts are read
    let notifs_a = NOTIFICATION_STORE.get_user_notifications(user_a, None, None, false);
    assert_eq!(notifs_a.unread_count, 0);

    // User B's alerts must remain unread!
    let notifs_b = NOTIFICATION_STORE.get_user_notifications(user_b, None, None, false);
    assert_eq!(notifs_b.unread_count, 1);
    assert!(notifs_b.items[0].read_at.is_none());
}

#[tokio::test]
async fn test_19_unauthorized_requests_rejected() {
    let server = setup_test_app();

    // No authorization header
    let res1 = server.get("/me/notifications").await;
    assert_eq!(res1.status_code(), StatusCode::UNAUTHORIZED);

    let res2 = server.post("/companies/shonen-labs-19/follow").await;
    assert_eq!(res2.status_code(), StatusCode::UNAUTHORIZED);

    let res3 = server.delete("/companies/shonen-labs-19/follow").await;
    assert_eq!(res3.status_code(), StatusCode::UNAUTHORIZED);
}

// ============================================================================
// GROUP 4: READ STATUS, FILTERING & PAGINATION (Tests 20-27)
// ============================================================================

#[tokio::test]
async fn test_20_mark_single_notification_read() {
    let server = setup_test_app();
    let user = "0x20a";
    let token = create_auth_token(user);

    NOTIFICATION_STORE
        .dispatch_notification(&[user.to_string()], NotificationType::NewReview, json!({}))
        .unwrap();

    let list_before = NOTIFICATION_STORE.get_user_notifications(user, None, None, false);
    let notif_id = list_before.items[0].id;
    assert!(list_before.items[0].read_at.is_none());

    let res = server
        .patch(&format!("/me/notifications/{}", notif_id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: MarkReadResponse = res.json();
    assert!(body.notification.read_at.is_some());
    assert_eq!(body.notification.id, notif_id);

    // Unread count is now 0
    let list_after = NOTIFICATION_STORE.get_user_notifications(user, None, None, false);
    assert_eq!(list_after.unread_count, 0);
}

#[tokio::test]
async fn test_21_mark_already_read_notification_is_idempotent() {
    let user = "0x21a";

    NOTIFICATION_STORE
        .dispatch_notification(&[user.to_string()], NotificationType::Mention, json!({}))
        .unwrap();

    let list = NOTIFICATION_STORE.get_user_notifications(user, None, None, false);
    let id = list.items[0].id;

    let res1 = NOTIFICATION_STORE.mark_as_read(user, id);
    assert!(res1.is_ok());

    let res2 = NOTIFICATION_STORE.mark_as_read(user, id);
    assert!(res2.is_ok());
    assert_eq!(res1.unwrap().read_at, res2.unwrap().read_at);
}

#[tokio::test]
async fn test_22_mark_nonexistent_notification_returns_404() {
    let server = setup_test_app();
    let user = "0x22a";
    let token = create_auth_token(user);
    let random_id = Uuid::new_v4();

    let res = server
        .patch(&format!("/me/notifications/{}", random_id))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_23_only_unread_filter() {
    let server = setup_test_app();
    let user = "0x23a";
    let token = create_auth_token(user);

    // Dispatch 4 notifications
    for _ in 0..4 {
        NOTIFICATION_STORE
            .dispatch_notification(&[user.to_string()], NotificationType::NewSummary, json!({}))
            .unwrap();
    }

    let all_notifs = NOTIFICATION_STORE.get_user_notifications(user, None, None, false);
    assert_eq!(all_notifs.total_count, 4);

    // Mark 1 as read
    NOTIFICATION_STORE
        .mark_as_read(user, all_notifs.items[0].id)
        .unwrap();

    // Query with only_unread=true
    let res = server
        .get("/me/notifications?only_unread=true")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: NotificationsListRes = res.json();
    assert_eq!(body.items.len(), 3);
    assert_eq!(body.unread_count, 3);
    for item in body.items {
        assert!(item.read_at.is_none());
    }
}

#[tokio::test]
async fn test_24_cursor_pagination_first_page() {
    let server = setup_test_app();
    let user = "0x24a";
    let token = create_auth_token(user);

    for i in 1..=5 {
        NOTIFICATION_STORE
            .dispatch_notification(
                &[user.to_string()],
                NotificationType::NewReview,
                json!({ "index": i }),
            )
            .unwrap();
    }

    // Limit 2
    let res = server
        .get("/me/notifications?limit=2")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: NotificationsListRes = res.json();
    assert_eq!(body.items.len(), 2);
    assert_eq!(body.total_count, 5);
    assert!(body.has_more);
    assert!(body.next_cursor.is_some());
}

#[tokio::test]
async fn test_25_cursor_pagination_traversal() {
    let server = setup_test_app();
    let user = "0x25a";
    let token = create_auth_token(user);

    for i in 1..=5 {
        NOTIFICATION_STORE
            .dispatch_notification(
                &[user.to_string()],
                NotificationType::NewReview,
                json!({ "index": i }),
            )
            .unwrap();
    }

    // Page 1
    let res1 = server
        .get("/me/notifications?limit=2")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    let page1: NotificationsListRes = res1.json();
    let cursor = page1.next_cursor.unwrap();

    // Page 2
    let res2 = server
        .get(&format!("/me/notifications?limit=2&cursor={}", cursor))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    let page2: NotificationsListRes = res2.json();
    assert_eq!(page2.items.len(), 2);
    assert!(page2.has_more);

    // Verify disjoint items
    let page1_ids: Vec<Uuid> = page1.items.iter().map(|n| n.id).collect();
    let page2_ids: Vec<Uuid> = page2.items.iter().map(|n| n.id).collect();
    for id in page2_ids {
        assert!(!page1_ids.contains(&id));
    }
}

#[tokio::test]
async fn test_26_cursor_pagination_last_page() {
    let server = setup_test_app();
    let user = "0x26a";
    let token = create_auth_token(user);

    for i in 1..=3 {
        NOTIFICATION_STORE
            .dispatch_notification(
                &[user.to_string()],
                NotificationType::NewReview,
                json!({ "index": i }),
            )
            .unwrap();
    }

    // Page 1 with limit 2
    let res1 = server
        .get("/me/notifications?limit=2")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    let page1: NotificationsListRes = res1.json();
    let cursor = page1.next_cursor.unwrap();

    // Page 2 (last page, contains remaining 1 item)
    let res2 = server
        .get(&format!("/me/notifications?limit=2&cursor={}", cursor))
        .add_header("Authorization", format!("Bearer {}", token))
        .await;
    let page2: NotificationsListRes = res2.json();
    assert_eq!(page2.items.len(), 1);
    assert!(!page2.has_more);
    assert!(page2.next_cursor.is_none());
}

#[tokio::test]
async fn test_27_mark_all_as_read_success() {
    let server = setup_test_app();
    let user = "0x27a";
    let token = create_auth_token(user);

    // Dispatch 5 notifications
    for _ in 0..5 {
        NOTIFICATION_STORE
            .dispatch_notification(&[user.to_string()], NotificationType::Mention, json!({}))
            .unwrap();
    }

    let before = NOTIFICATION_STORE.get_user_notifications(user, None, None, false);
    assert_eq!(before.unread_count, 5);

    let res = server
        .post("/me/notifications/read-all")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: MarkAllReadResponse = res.json();
    assert_eq!(body.updated_count, 5);

    let after = NOTIFICATION_STORE.get_user_notifications(user, None, None, false);
    assert_eq!(after.unread_count, 0);
}
