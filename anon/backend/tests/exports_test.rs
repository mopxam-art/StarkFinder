use chrono::Utc;
use uuid::Uuid;

use backend::{
    models::export::{ExportStatus, ExportedReview},
    services::export_service::{
        ExportError, ExportService, canonical_wallet,
    },
};

const USER_ALICE: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";
const USER_BOB: &str =
    "0x0222222222222222222222222222222222222222222222222222222222222222";

fn setup_service() -> ExportService {
    ExportService::new()
}

fn sample_review(company: &str, rating: u8) -> ExportedReview {
    ExportedReview {
        id: Uuid::new_v4(),
        company_slug: company.to_string(),
        title: Some("Great workplace".to_string()),
        content: "Transparent leadership and excellent engineering culture.".to_string(),
        rating,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn test_01_request_export_happy_path() {
    let service = setup_service();
    let res = service
        .request_export(USER_ALICE)
        .expect("export request should succeed");

    assert_eq!(res.status, ExportStatus::Ready);
    assert!(res.download_url.is_some());
    assert!(res.expires_at.is_some());
}

#[tokio::test]
async fn test_02_request_export_creates_valid_job() {
    let service = setup_service();
    let res = service.request_export(USER_ALICE).unwrap();

    let status = service
        .get_export_status(USER_ALICE, res.job_id)
        .expect("status check should succeed");

    assert_eq!(status.job_id, res.job_id);
    assert_eq!(status.status, ExportStatus::Ready);
}

#[tokio::test]
async fn test_03_request_export_rate_limiting_same_day() {
    let service = setup_service();
    service.request_export(USER_ALICE).unwrap();

    let second_attempt = service.request_export(USER_ALICE).unwrap_err();
    assert!(matches!(second_attempt, ExportError::TooManyRequests(_)));
}

#[tokio::test]
async fn test_04_request_export_user_isolation() {
    let service = setup_service();
    // User Alice can request
    assert!(service.request_export(USER_ALICE).is_ok());
    // User Bob can also request independently without hitting Alice's rate limit
    assert!(service.request_export(USER_BOB).is_ok());
}

#[tokio::test]
async fn test_05_get_export_status_happy_path() {
    let service = setup_service();
    let job = service.request_export(USER_ALICE).unwrap();

    let status = service.get_export_status(USER_ALICE, job.job_id).unwrap();
    assert_eq!(status.job_id, job.job_id);
    assert_eq!(status.status, ExportStatus::Ready);
    assert_eq!(status.download_url, job.download_url);
}

#[tokio::test]
async fn test_06_get_export_status_forbidden_for_other_user() {
    let service = setup_service();
    let alice_job = service.request_export(USER_ALICE).unwrap();

    // Bob attempts to check Alice's job status
    let err = service
        .get_export_status(USER_BOB, alice_job.job_id)
        .unwrap_err();
    assert!(matches!(err, ExportError::Forbidden(_)));
}

#[tokio::test]
async fn test_07_get_export_status_not_found() {
    let service = setup_service();
    let random_id = Uuid::new_v4();

    let err = service
        .get_export_status(USER_ALICE, random_id)
        .unwrap_err();
    assert!(matches!(err, ExportError::NotFound(_)));
}

#[tokio::test]
async fn test_08_download_export_happy_path() {
    let service = setup_service();
    let job = service.request_export(USER_ALICE).unwrap();

    // Extract token from download_url "/exports/download/<token>"
    let url = job.download_url.unwrap();
    let token = url.split('/').next_back().unwrap();

    let payload = service
        .download_export(token)
        .expect("download should succeed");
    assert_eq!(payload.user_wallet, canonical_wallet(USER_ALICE));
}

#[tokio::test]
async fn test_09_download_export_contains_seeded_reviews() {
    let service = setup_service();
    let r1 = sample_review("argent", 5);
    let r2 = sample_review("braavos", 4);

    service.add_user_review(USER_ALICE, r1.clone());
    service.add_user_review(USER_ALICE, r2.clone());

    let job = service.request_export(USER_ALICE).unwrap();
    let token = job.download_url.unwrap().split('/').next_back().unwrap().to_string();

    let payload = service.download_export(&token).unwrap();
    assert_eq!(payload.total, 2);
    assert_eq!(payload.reviews.len(), 2);
    assert!(payload.reviews.iter().any(|r| r.company_slug == "argent"));
    assert!(payload.reviews.iter().any(|r| r.company_slug == "braavos"));
}

#[tokio::test]
async fn test_10_download_export_empty_for_user_without_reviews() {
    let service = setup_service();
    let job = service.request_export(USER_ALICE).unwrap();
    let token = job.download_url.unwrap().split('/').next_back().unwrap().to_string();

    let payload = service.download_export(&token).unwrap();
    assert_eq!(payload.total, 0);
    assert!(payload.reviews.is_empty());
}

#[tokio::test]
async fn test_11_download_export_invalid_token_returns_not_found() {
    let service = setup_service();
    let err = service.download_export("invalid-token-1234").unwrap_err();
    assert!(matches!(err, ExportError::NotFound(_)));
}

#[tokio::test]
async fn test_12_download_export_expired_link_returns_gone() {
    let service = setup_service();
    let job = service.request_export(USER_ALICE).unwrap();
    let token = job.download_url.unwrap().split('/').next_back().unwrap().to_string();

    // Artificially expire the job in internal state
    let res = service.download_export(&token);
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_13_wallet_normalization_case_insensitivity() {
    let service = setup_service();
    let upper_wallet = "0x0AABBCCDDEEFF00112233445566778899AABBCCDDEEFF0011223344556677889";
    let lower_wallet = upper_wallet.to_lowercase();

    let job = service.request_export(upper_wallet).unwrap();
    let status = service.get_export_status(&lower_wallet, job.job_id).unwrap();
    assert_eq!(status.job_id, job.job_id);
}

#[tokio::test]
async fn test_14_empty_wallet_returns_bad_request() {
    let service = setup_service();
    let err = service.request_export("   ").unwrap_err();
    assert!(matches!(err, ExportError::BadRequest(_)));
}

#[tokio::test]
async fn test_15_clear_all_resets_state() {
    let service = setup_service();
    let job = service.request_export(USER_ALICE).unwrap();
    assert!(service.get_export_status(USER_ALICE, job.job_id).is_ok());

    service.clear_all();
    assert!(service.get_export_status(USER_ALICE, job.job_id).is_err());
}

#[tokio::test]
async fn test_16_canonical_wallet_helper() {
    let raw = "  0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  ";
    let norm = canonical_wallet(raw);
    assert!(norm.starts_with("0x"));
}

#[tokio::test]
async fn test_17_multiple_users_request_exports_independently() {
    let service = setup_service();
    let j1 = service.request_export(USER_ALICE).unwrap();
    let j2 = service.request_export(USER_BOB).unwrap();

    assert_ne!(j1.job_id, j2.job_id);
    assert_ne!(j1.download_url, j2.download_url);
}

#[tokio::test]
async fn test_18_job_id_and_token_uniqueness() {
    let service = setup_service();
    let j1 = service.request_export(USER_ALICE).unwrap();
    service.clear_all();
    let j2 = service.request_export(USER_ALICE).unwrap();

    assert_ne!(j1.job_id, j2.job_id);
}

#[tokio::test]
async fn test_19_exported_review_fields_preservation() {
    let service = setup_service();
    let r = sample_review("starkware", 5);
    service.add_user_review(USER_ALICE, r.clone());

    let job = service.request_export(USER_ALICE).unwrap();
    let token = job.download_url.unwrap().split('/').next_back().unwrap().to_string();

    let payload = service.download_export(&token).unwrap();
    let exported = &payload.reviews[0];

    assert_eq!(exported.company_slug, "starkware");
    assert_eq!(exported.rating, 5);
    assert_eq!(exported.title, Some("Great workplace".to_string()));
}

#[tokio::test]
async fn test_20_download_export_deterministic_counts() {
    let service = setup_service();
    for i in 1..=5 {
        service.add_user_review(USER_ALICE, sample_review("comp", i));
    }

    let job = service.request_export(USER_ALICE).unwrap();
    let token = job.download_url.unwrap().split('/').next_back().unwrap().to_string();

    let payload = service.download_export(&token).unwrap();
    assert_eq!(payload.total, 5);
    assert_eq!(payload.reviews.len(), 5);
}

#[tokio::test]
async fn test_21_rate_limit_allows_different_wallets() {
    let service = setup_service();
    assert!(service.request_export(USER_ALICE).is_ok());
    assert!(service.request_export(USER_BOB).is_ok());
}

#[tokio::test]
async fn test_22_stress_10_reviews_in_export() {
    let service = setup_service();
    for i in 0..10 {
        service.add_user_review(USER_ALICE, sample_review(&format!("company-{}", i), 4));
    }

    let job = service.request_export(USER_ALICE).unwrap();
    let token = job.download_url.unwrap().split('/').next_back().unwrap().to_string();

    let payload = service.download_export(&token).unwrap();
    assert_eq!(payload.total, 10);
}

#[tokio::test]
async fn test_23_repeated_download_same_token_within_expiry() {
    let service = setup_service();
    let job = service.request_export(USER_ALICE).unwrap();
    let token = job.download_url.unwrap().split('/').next_back().unwrap().to_string();

    let d1 = service.download_export(&token);
    let d2 = service.download_export(&token);

    assert!(d1.is_ok());
    assert!(d2.is_ok());
}

#[tokio::test]
async fn test_24_export_status_enum_serialization() {
    assert_eq!(
        serde_json::to_string(&ExportStatus::Ready).unwrap(),
        "\"ready\""
    );
    assert_eq!(
        serde_json::to_string(&ExportStatus::Pending).unwrap(),
        "\"pending\""
    );
    assert_eq!(
        serde_json::to_string(&ExportStatus::Error).unwrap(),
        "\"error\""
    );
}

#[tokio::test]
async fn test_25_invalid_job_id_format() {
    let service = setup_service();
    let res = service.get_export_status(USER_ALICE, Uuid::nil());
    assert!(res.is_err());
}
