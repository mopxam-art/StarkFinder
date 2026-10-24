use uuid::Uuid;

use backend::{
    models::appeal::{
        AppealActor, AppealStatus, CreateAppealRequest, ResolveAppealRequest,
    },
    services::appeal_service::{
        AppealError, AppealService, MAX_NOTE_LEN, MAX_REASON_LEN, canonical_wallet,
    },
};

const USER_ALICE: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";
const USER_BOB: &str =
    "0x0222222222222222222222222222222222222222222222222222222222222222";

fn setup_service() -> AppealService {
    AppealService::new()
}

fn sample_create_request(review_id: Uuid, actor: AppealActor) -> CreateAppealRequest {
    CreateAppealRequest {
        review_id,
        actor,
        reason: "The moderation decision was incorrect because this review reflects verified employment experience.".to_string(),
    }
}

#[tokio::test]
async fn test_01_create_appeal_by_author_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    let req = sample_create_request(review_id, AppealActor::Author);

    let appeal = service
        .create_appeal(USER_ALICE, req)
        .expect("author appeal creation should succeed");

    assert_eq!(appeal.review_id, review_id);
    assert_eq!(appeal.actor, AppealActor::Author);
    assert_eq!(appeal.appellant_wallet, canonical_wallet(USER_ALICE));
    assert_eq!(appeal.status, AppealStatus::Open);
    assert!(appeal.resolution_note.is_none());
    assert!(appeal.resolved_at.is_none());
}

#[tokio::test]
async fn test_02_create_appeal_by_owner_happy_path() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    let req = sample_create_request(review_id, AppealActor::Owner);

    let appeal = service
        .create_appeal(USER_BOB, req)
        .expect("owner appeal creation should succeed");

    assert_eq!(appeal.review_id, review_id);
    assert_eq!(appeal.actor, AppealActor::Owner);
    assert_eq!(appeal.status, AppealStatus::Open);
}

#[tokio::test]
async fn test_03_create_appeal_validation_empty_wallet() {
    let service = setup_service();
    let req = sample_create_request(Uuid::new_v4(), AppealActor::Author);
    let err = service.create_appeal("   ", req).unwrap_err();
    assert!(matches!(err, AppealError::BadRequest(_)));
}

#[tokio::test]
async fn test_04_create_appeal_validation_reason_too_short() {
    let service = setup_service();
    let mut req = sample_create_request(Uuid::new_v4(), AppealActor::Author);
    req.reason = "short".to_string(); // 5 < 10
    let err = service.create_appeal(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, AppealError::BadRequest(_)));
}

#[tokio::test]
async fn test_05_create_appeal_validation_reason_too_long() {
    let service = setup_service();
    let mut req = sample_create_request(Uuid::new_v4(), AppealActor::Author);
    req.reason = "A".repeat(MAX_REASON_LEN + 1);
    let err = service.create_appeal(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, AppealError::BadRequest(_)));
}

#[tokio::test]
async fn test_06_create_appeal_duplicate_open_appeal_conflict() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    let req1 = sample_create_request(review_id, AppealActor::Author);
    service.create_appeal(USER_ALICE, req1).unwrap();

    let req2 = sample_create_request(review_id, AppealActor::Author);
    let err = service.create_appeal(USER_ALICE, req2).unwrap_err();
    assert!(matches!(err, AppealError::Conflict(_)));
}

#[tokio::test]
async fn test_07_create_appeal_allows_different_actor_same_review() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    let req_author = sample_create_request(review_id, AppealActor::Author);
    let req_owner = sample_create_request(review_id, AppealActor::Owner);

    assert!(service.create_appeal(USER_ALICE, req_author).is_ok());
    assert!(service.create_appeal(USER_BOB, req_owner).is_ok());
}

#[tokio::test]
async fn test_08_create_appeal_sanitizes_xss_in_reason() {
    let service = setup_service();
    let mut req = sample_create_request(Uuid::new_v4(), AppealActor::Author);
    req.reason = "Reason with <script>alert('xss')</script> and javascript:void(0).".to_string();

    let appeal = service.create_appeal(USER_ALICE, req).unwrap();
    assert!(!appeal.reason.contains("<script>"));
    assert!(appeal.reason.contains("&lt;script&gt;"));
    assert!(!appeal.reason.contains("javascript:"));
}

#[tokio::test]
async fn test_09_get_appeal_happy_path() {
    let service = setup_service();
    let req = sample_create_request(Uuid::new_v4(), AppealActor::Author);
    let created = service.create_appeal(USER_ALICE, req).unwrap();

    let fetched = service
        .get_appeal(created.id)
        .expect("should find appeal by id");
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.reason, created.reason);
}

#[tokio::test]
async fn test_10_get_appeal_not_found() {
    let service = setup_service();
    let err = service.get_appeal(Uuid::new_v4()).unwrap_err();
    assert!(matches!(err, AppealError::NotFound(_)));
}

#[tokio::test]
async fn test_11_list_appeals_empty() {
    let service = setup_service();
    let res = service.list_appeals(None).unwrap();
    assert_eq!(res.total, 0);
    assert!(res.items.is_empty());
}

#[tokio::test]
async fn test_12_list_appeals_all() {
    let service = setup_service();
    service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();
    service
        .create_appeal(
            USER_BOB,
            sample_create_request(Uuid::new_v4(), AppealActor::Owner),
        )
        .unwrap();

    let res = service.list_appeals(None).unwrap();
    assert_eq!(res.total, 2);
    assert_eq!(res.items.len(), 2);
}

#[tokio::test]
async fn test_13_list_appeals_filter_by_open() {
    let service = setup_service();
    let a1 = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();
    let a2 = service
        .create_appeal(
            USER_BOB,
            sample_create_request(Uuid::new_v4(), AppealActor::Owner),
        )
        .unwrap();

    // Resolve a2 as Accepted
    service
        .resolve_appeal(
            a2.id,
            ResolveAppealRequest {
                status: AppealStatus::Accepted,
                resolution_note: "Review restored after evidence check.".to_string(),
            },
        )
        .unwrap();

    let open_list = service.list_appeals(Some(AppealStatus::Open)).unwrap();
    assert_eq!(open_list.total, 1);
    assert_eq!(open_list.items[0].id, a1.id);
}

#[tokio::test]
async fn test_14_list_appeals_filter_by_accepted() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Accepted,
                resolution_note: "Approved by moderator.".to_string(),
            },
        )
        .unwrap();

    let accepted_list = service.list_appeals(Some(AppealStatus::Accepted)).unwrap();
    assert_eq!(accepted_list.total, 1);
    assert_eq!(accepted_list.items[0].status, AppealStatus::Accepted);
}

#[tokio::test]
async fn test_15_list_appeals_filter_by_rejected() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Rejected,
                resolution_note: "Rejected due to terms of service violation.".to_string(),
            },
        )
        .unwrap();

    let rejected_list = service.list_appeals(Some(AppealStatus::Rejected)).unwrap();
    assert_eq!(rejected_list.total, 1);
    assert_eq!(rejected_list.items[0].status, AppealStatus::Rejected);
}

#[tokio::test]
async fn test_16_list_appeals_sorted_by_created_desc() {
    let service = setup_service();
    for _ in 0..5 {
        service
            .create_appeal(
                USER_ALICE,
                sample_create_request(Uuid::new_v4(), AppealActor::Author),
            )
            .unwrap();
    }

    let res = service.list_appeals(None).unwrap();
    assert_eq!(res.total, 5);
    for w in res.items.windows(2) {
        assert!(w[0].created_at >= w[1].created_at);
    }
}

#[tokio::test]
async fn test_17_resolve_appeal_accept_happy_path() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    let resolved = service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Accepted,
                resolution_note: "Sufficient proof of employment verified.".to_string(),
            },
        )
        .expect("resolution should succeed");

    assert_eq!(resolved.status, AppealStatus::Accepted);
    assert_eq!(
        resolved.resolution_note.as_deref(),
        Some("Sufficient proof of employment verified.")
    );
    assert!(resolved.resolved_at.is_some());
}

#[tokio::test]
async fn test_18_resolve_appeal_reject_happy_path() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    let resolved = service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Rejected,
                resolution_note: "Appeal denied after secondary review.".to_string(),
            },
        )
        .expect("rejection should succeed");

    assert_eq!(resolved.status, AppealStatus::Rejected);
    assert!(resolved.resolved_at.is_some());
}

#[tokio::test]
async fn test_19_resolve_appeal_rejects_open_status() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    let err = service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Open,
                resolution_note: "Attempt to resolve as open".to_string(),
            },
        )
        .unwrap_err();

    assert!(matches!(err, AppealError::BadRequest(_)));
}

#[tokio::test]
async fn test_20_resolve_appeal_note_too_short() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    let err = service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Accepted,
                resolution_note: "tiny".to_string(), // 4 < 5
            },
        )
        .unwrap_err();

    assert!(matches!(err, AppealError::BadRequest(_)));
}

#[tokio::test]
async fn test_21_resolve_appeal_note_too_long() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    let err = service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Accepted,
                resolution_note: "A".repeat(MAX_NOTE_LEN + 1),
            },
        )
        .unwrap_err();

    assert!(matches!(err, AppealError::BadRequest(_)));
}

#[tokio::test]
async fn test_22_resolve_appeal_sanitizes_xss_in_note() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    let resolved = service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Accepted,
                resolution_note: "Decision noted: <script>bad()</script> verified.".to_string(),
            },
        )
        .unwrap();

    assert!(!resolved.resolution_note.as_ref().unwrap().contains("<script>"));
    assert!(resolved.resolution_note.as_ref().unwrap().contains("&lt;script&gt;"));
}

#[tokio::test]
async fn test_23_resolve_appeal_already_resolved_conflict() {
    let service = setup_service();
    let a = service
        .create_appeal(
            USER_ALICE,
            sample_create_request(Uuid::new_v4(), AppealActor::Author),
        )
        .unwrap();

    service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Accepted,
                resolution_note: "First resolution note.".to_string(),
            },
        )
        .unwrap();

    // Second resolution attempt should fail
    let err = service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Rejected,
                resolution_note: "Second attempt note.".to_string(),
            },
        )
        .unwrap_err();

    assert!(matches!(err, AppealError::BadRequest(_)));
}

#[tokio::test]
async fn test_24_resolve_appeal_not_found() {
    let service = setup_service();
    let err = service
        .resolve_appeal(
            Uuid::new_v4(),
            ResolveAppealRequest {
                status: AppealStatus::Accepted,
                resolution_note: "Resolution for nonexistent appeal".to_string(),
            },
        )
        .unwrap_err();

    assert!(matches!(err, AppealError::NotFound(_)));
}

#[tokio::test]
async fn test_25_allows_new_appeal_after_previous_resolved() {
    let service = setup_service();
    let review_id = Uuid::new_v4();
    let a = service
        .create_appeal(USER_ALICE, sample_create_request(review_id, AppealActor::Author))
        .unwrap();

    // Resolve the first appeal
    service
        .resolve_appeal(
            a.id,
            ResolveAppealRequest {
                status: AppealStatus::Rejected,
                resolution_note: "Initial rejection.".to_string(),
            },
        )
        .unwrap();

    // Now a new appeal on the same review can be submitted because the prior is no longer Open
    let req2 = sample_create_request(review_id, AppealActor::Author);
    assert!(service.create_appeal(USER_ALICE, req2).is_ok());
}

#[tokio::test]
async fn test_26_appeal_actor_enum_serde() {
    assert_eq!(
        serde_json::to_string(&AppealActor::Author).unwrap(),
        "\"author\""
    );
    assert_eq!(
        serde_json::to_string(&AppealActor::Owner).unwrap(),
        "\"owner\""
    );

    let parsed: AppealActor = serde_json::from_str("\"author\"").unwrap();
    assert_eq!(parsed, AppealActor::Author);
}

#[tokio::test]
async fn test_27_appeal_status_enum_serde() {
    assert_eq!(
        serde_json::to_string(&AppealStatus::Open).unwrap(),
        "\"open\""
    );
    assert_eq!(
        serde_json::to_string(&AppealStatus::Accepted).unwrap(),
        "\"accepted\""
    );
    assert_eq!(
        serde_json::to_string(&AppealStatus::Rejected).unwrap(),
        "\"rejected\""
    );

    let parsed: AppealStatus = serde_json::from_str("\"accepted\"").unwrap();
    assert_eq!(parsed, AppealStatus::Accepted);
}

#[tokio::test]
async fn test_28_concurrent_appeals_isolation() {
    let service = setup_service();
    let r1 = Uuid::new_v4();
    let r2 = Uuid::new_v4();

    let a1 = service
        .create_appeal(USER_ALICE, sample_create_request(r1, AppealActor::Author))
        .unwrap();
    let a2 = service
        .create_appeal(USER_BOB, sample_create_request(r2, AppealActor::Owner))
        .unwrap();

    assert_ne!(a1.id, a2.id);
    assert_eq!(service.list_appeals(None).unwrap().total, 2);
}
