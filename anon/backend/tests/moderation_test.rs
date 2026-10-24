use uuid::Uuid;

use backend::{
    models::moderation::{
        ReviewModerationStatus, UpdateReviewStatusRequest,
    },
    services::moderation_service::{
        MAX_REASON_LEN, ModerationError, ModerationService, is_valid_transition,
    },
};

const ADMIN_ALICE: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";
const NON_ADMIN_BOB: &str =
    "0x0222222222222222222222222222222222222222222222222222222222222222";

fn setup_service() -> ModerationService {
    let service = ModerationService::new();
    service.add_admin(ADMIN_ALICE);
    service
}

#[tokio::test]
async fn test_01_transition_published_to_needs_review_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Published);

    let res = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: Some("Reported for potential PII".to_string()),
            },
        )
        .expect("transition should succeed");

    assert_eq!(res.prev_status, ReviewModerationStatus::Published);
    assert_eq!(res.current_status, ReviewModerationStatus::NeedsReview);
    assert_eq!(
        service.get_review_status(review_id),
        ReviewModerationStatus::NeedsReview
    );
}

#[tokio::test]
async fn test_02_transition_needs_review_to_published_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::NeedsReview);

    let res = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Published,
                reason: Some("No violation found".to_string()),
            },
        )
        .unwrap();

    assert_eq!(res.prev_status, ReviewModerationStatus::NeedsReview);
    assert_eq!(res.current_status, ReviewModerationStatus::Published);
}

#[tokio::test]
async fn test_03_transition_needs_review_to_blocked_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::NeedsReview);

    let res = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Blocked,
                reason: Some("Content violates moderation policy".to_string()),
            },
        )
        .unwrap();

    assert_eq!(res.current_status, ReviewModerationStatus::Blocked);
}

#[tokio::test]
async fn test_04_transition_blocked_to_needs_review_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Blocked);

    let res = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: Some("Author submitted appeal with new context".to_string()),
            },
        )
        .unwrap();

    assert_eq!(res.current_status, ReviewModerationStatus::NeedsReview);
}

#[tokio::test]
async fn test_05_transition_published_to_deleted_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Published);

    let res = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Deleted,
                reason: Some("Author requested erasure".to_string()),
            },
        )
        .unwrap();

    assert_eq!(res.current_status, ReviewModerationStatus::Deleted);
}

#[tokio::test]
async fn test_06_transition_needs_review_to_deleted_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::NeedsReview);

    let res = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Deleted,
                reason: Some("Severe terms breach".to_string()),
            },
        )
        .unwrap();

    assert_eq!(res.current_status, ReviewModerationStatus::Deleted);
}

#[tokio::test]
async fn test_07_transition_blocked_to_deleted_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Blocked);

    let res = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Deleted,
                reason: Some("Permanent purge".to_string()),
            },
        )
        .unwrap();

    assert_eq!(res.current_status, ReviewModerationStatus::Deleted);
}

#[tokio::test]
async fn test_08_transition_from_deleted_is_terminal_fails() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Deleted);

    let err = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Published,
                reason: Some("Attempt to revive".to_string()),
            },
        )
        .unwrap_err();

    assert!(matches!(err, ModerationError::BadRequest(_)));
}

#[tokio::test]
async fn test_09_transition_to_same_status_fails() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Published);

    let err = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Published,
                reason: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, ModerationError::BadRequest(_)));
}

#[tokio::test]
async fn test_10_transition_blocked_to_published_direct_fails() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Blocked);

    let err = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Published,
                reason: Some("Direct publish skipping needs_review".to_string()),
            },
        )
        .unwrap_err();

    assert!(matches!(err, ModerationError::BadRequest(_)));
}

#[tokio::test]
async fn test_11_transition_published_to_blocked_direct_fails() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Published);

    let err = service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Blocked,
                reason: Some("Direct block skipping needs_review".to_string()),
            },
        )
        .unwrap_err();

    assert!(matches!(err, ModerationError::BadRequest(_)));
}

#[tokio::test]
async fn test_12_non_admin_transition_forbidden() {
    let service = setup_service();
    let review_id = Uuid::new_v4();

    let err = service
        .transition_status(
            NON_ADMIN_BOB,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, ModerationError::Forbidden(_)));
}

#[tokio::test]
async fn test_13_audit_log_created_on_transition() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    service.seed_review_status(review_id, ReviewModerationStatus::Published);

    service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: Some("First flag".to_string()),
            },
        )
        .unwrap();

    let log = service.get_audit_log(ADMIN_ALICE, Some(review_id)).unwrap();
    assert_eq!(log.total, 1);
    assert_eq!(log.items[0].review_id, review_id);
    assert_eq!(log.items[0].prev_status, ReviewModerationStatus::Published);
    assert_eq!(log.items[0].new_status, ReviewModerationStatus::NeedsReview);
}

#[tokio::test]
async fn test_14_audit_log_entry_contains_reason_and_admin() {
    let service = setup_service();
    let review_id = Uuid::new_v4();

    service
        .transition_status(
            ADMIN_ALICE,
            review_id,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: Some("Legal review requested".to_string()),
            },
        )
        .unwrap();

    let log = service.get_audit_log(ADMIN_ALICE, Some(review_id)).unwrap();
    assert_eq!(
        log.items[0].reason.as_deref(),
        Some("Legal review requested")
    );
    assert_eq!(
        log.items[0].admin_wallet,
        backend::services::moderation_service::canonical_wallet(ADMIN_ALICE)
    );
}

#[tokio::test]
async fn test_15_audit_log_filters_by_review_id() {
    let service = setup_service();
    let r1 = Uuid::new_v4();
    let r2 = Uuid::new_v4();

    service
        .transition_status(
            ADMIN_ALICE,
            r1,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: None,
            },
        )
        .unwrap();
    service
        .transition_status(
            ADMIN_ALICE,
            r2,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: None,
            },
        )
        .unwrap();

    let log_r1 = service.get_audit_log(ADMIN_ALICE, Some(r1)).unwrap();
    assert_eq!(log_r1.total, 1);
    assert_eq!(log_r1.items[0].review_id, r1);
}

#[tokio::test]
async fn test_16_audit_log_unfiltered_lists_all_entries() {
    let service = setup_service();
    let r1 = Uuid::new_v4();
    let r2 = Uuid::new_v4();

    service
        .transition_status(
            ADMIN_ALICE,
            r1,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: None,
            },
        )
        .unwrap();
    service
        .transition_status(
            ADMIN_ALICE,
            r2,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: None,
            },
        )
        .unwrap();

    let all_logs = service.get_audit_log(ADMIN_ALICE, None).unwrap();
    assert_eq!(all_logs.total, 2);
}

#[tokio::test]
async fn test_17_audit_log_sorted_chronological_ascending() {
    let service = setup_service();
    let r = Uuid::new_v4();

    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: None,
            },
        )
        .unwrap();
    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Blocked,
                reason: None,
            },
        )
        .unwrap();

    let log = service.get_audit_log(ADMIN_ALICE, Some(r)).unwrap();
    assert_eq!(log.total, 2);
    assert!(log.items[0].at <= log.items[1].at);
}

#[tokio::test]
async fn test_18_non_admin_get_audit_log_forbidden() {
    let service = setup_service();
    let err = service.get_audit_log(NON_ADMIN_BOB, None).unwrap_err();
    assert!(matches!(err, ModerationError::Forbidden(_)));
}

#[tokio::test]
async fn test_19_reason_sanitizes_xss() {
    let service = setup_service();
    let r = Uuid::new_v4();

    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: Some("<script>alert('pwn')</script> flagged".to_string()),
            },
        )
        .unwrap();

    let log = service.get_audit_log(ADMIN_ALICE, Some(r)).unwrap();
    let reason = log.items[0].reason.as_ref().unwrap();
    assert!(!reason.contains("<script>"));
    assert!(reason.contains("&lt;script&gt;"));
}

#[tokio::test]
async fn test_20_reason_exceeds_max_len_bad_request() {
    let service = setup_service();
    let r = Uuid::new_v4();
    let long_reason = "A".repeat(MAX_REASON_LEN + 1);

    let err = service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: Some(long_reason),
            },
        )
        .unwrap_err();

    assert!(matches!(err, ModerationError::BadRequest(_)));
}

#[tokio::test]
async fn test_21_case_insensitive_admin_wallet() {
    let service = setup_service();
    let r = Uuid::new_v4();
    let uppercase_admin = ADMIN_ALICE.to_uppercase();

    let res = service.transition_status(
        &uppercase_admin,
        r,
        UpdateReviewStatusRequest {
            status: ReviewModerationStatus::NeedsReview,
            reason: None,
        },
    );
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_22_review_moderation_status_enum_serde() {
    assert_eq!(
        serde_json::to_string(&ReviewModerationStatus::Published).unwrap(),
        "\"published\""
    );
    assert_eq!(
        serde_json::to_string(&ReviewModerationStatus::NeedsReview).unwrap(),
        "\"needs_review\""
    );
    assert_eq!(
        serde_json::to_string(&ReviewModerationStatus::Blocked).unwrap(),
        "\"blocked\""
    );
    assert_eq!(
        serde_json::to_string(&ReviewModerationStatus::Deleted).unwrap(),
        "\"deleted\""
    );

    let parsed: ReviewModerationStatus = serde_json::from_str("\"needs_review\"").unwrap();
    assert_eq!(parsed, ReviewModerationStatus::NeedsReview);
}

#[tokio::test]
async fn test_23_full_state_machine_lifecycle() {
    let service = setup_service();
    let r = Uuid::new_v4();

    // 1. Published -> NeedsReview
    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: Some("Step 1".to_string()),
            },
        )
        .unwrap();

    // 2. NeedsReview -> Blocked
    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Blocked,
                reason: Some("Step 2".to_string()),
            },
        )
        .unwrap();

    // 3. Blocked -> NeedsReview
    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: Some("Step 3".to_string()),
            },
        )
        .unwrap();

    // 4. NeedsReview -> Published
    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Published,
                reason: Some("Step 4".to_string()),
            },
        )
        .unwrap();

    // 5. Published -> Deleted
    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::Deleted,
                reason: Some("Step 5".to_string()),
            },
        )
        .unwrap();

    assert_eq!(service.get_review_status(r), ReviewModerationStatus::Deleted);
    let log = service.get_audit_log(ADMIN_ALICE, Some(r)).unwrap();
    assert_eq!(log.total, 5);
}

#[tokio::test]
async fn test_24_is_admin_check() {
    let service = setup_service();
    assert!(service.is_admin(ADMIN_ALICE));
    assert!(!service.is_admin(NON_ADMIN_BOB));
}

#[tokio::test]
async fn test_25_clear_all_resets_state() {
    let service = setup_service();
    let r = Uuid::new_v4();
    service
        .transition_status(
            ADMIN_ALICE,
            r,
            UpdateReviewStatusRequest {
                status: ReviewModerationStatus::NeedsReview,
                reason: None,
            },
        )
        .unwrap();

    service.clear_all();
    assert!(!service.is_admin(ADMIN_ALICE));
    assert_eq!(service.get_review_status(r), ReviewModerationStatus::Published);
}

#[tokio::test]
async fn test_26_is_valid_transition_exhaustive_helper() {
    assert!(is_valid_transition(
        ReviewModerationStatus::Published,
        ReviewModerationStatus::NeedsReview
    ));
    assert!(is_valid_transition(
        ReviewModerationStatus::NeedsReview,
        ReviewModerationStatus::Blocked
    ));
    assert!(!is_valid_transition(
        ReviewModerationStatus::Blocked,
        ReviewModerationStatus::Published
    ));
    assert!(!is_valid_transition(
        ReviewModerationStatus::Deleted,
        ReviewModerationStatus::Published
    ));
}
