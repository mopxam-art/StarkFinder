use uuid::Uuid;

use backend::{
    models::verify::{
        StartVerificationRequest, SubmitProofRequest, VerificationStatus,
    },
    services::verify_service::{
        MAX_PROOF_BLOB_BYTES, VerifyError, VerifyService,
    },
};

const USER_ALICE: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";
const USER_BOB: &str =
    "0x0222222222222222222222222222222222222222222222222222222222222222";

fn setup_service() -> VerifyService {
    VerifyService::new()
}

fn valid_proof_payload() -> String {
    "{\"zk_tls_proof\":\"0xabcdef1234567890abcdef1234567890abcdef1234567890\",\"valid\":true}".to_string()
}

#[tokio::test]
async fn test_01_start_verification_session_happy_path() {
    let service = setup_service();
    let req = StartVerificationRequest {
        company_slug: "starkware".to_string(),
        provider: None,
    };

    let res = service
        .start_session(USER_ALICE, req)
        .expect("start session should succeed");

    assert_eq!(res.company_slug, "starkware");
    assert!(res.challenge.starts_with("0x"));
    assert_eq!(res.challenge.len(), 66); // 0x + 64 hex chars
    assert!(res.expires_at > chrono::Utc::now());
    assert!(res.provider_session_url.contains("reclaim-provider.org"));
}

#[tokio::test]
async fn test_02_start_verification_custom_provider() {
    let service = setup_service();
    let req = StartVerificationRequest {
        company_slug: "argent".to_string(),
        provider: Some("opacity".to_string()),
    };

    let res = service.start_session(USER_ALICE, req).unwrap();
    assert!(res.provider_session_url.contains("opacity-provider.org"));
}

#[tokio::test]
async fn test_03_start_verification_empty_company_slug_bad_request() {
    let service = setup_service();
    let req = StartVerificationRequest {
        company_slug: "   ".to_string(),
        provider: None,
    };

    let err = service.start_session(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, VerifyError::BadRequest(_)));
}

#[tokio::test]
async fn test_04_start_verification_empty_wallet_bad_request() {
    let service = setup_service();
    let req = StartVerificationRequest {
        company_slug: "starkware".to_string(),
        provider: None,
    };

    let err = service.start_session("   ", req).unwrap_err();
    assert!(matches!(err, VerifyError::BadRequest(_)));
}

#[tokio::test]
async fn test_05_start_verification_challenge_is_valid_hex_and_66_chars() {
    let service = setup_service();
    let req = StartVerificationRequest {
        company_slug: "braavos".to_string(),
        provider: None,
    };

    let res = service.start_session(USER_ALICE, req).unwrap();
    assert!(res.challenge.starts_with("0x"));
    assert_eq!(res.challenge.len(), 66);
    let hex_part = &res.challenge[2..];
    assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
}

#[tokio::test]
async fn test_06_start_verification_expires_in_future() {
    let service = setup_service();
    let req = StartVerificationRequest {
        company_slug: "starkware".to_string(),
        provider: None,
    };

    let res = service.start_session(USER_ALICE, req).unwrap();
    let duration_secs = (res.expires_at - chrono::Utc::now()).num_seconds();
    assert!((800..=900).contains(&duration_secs)); // ~15 minutes
}

#[tokio::test]
async fn test_07_submit_proof_happy_path_verified() {
    let service = setup_service();
    let start_res = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    let submit_req = SubmitProofRequest {
        session_id: start_res.session_id,
        proof_blob: valid_proof_payload(),
        public_inputs: None,
    };

    let res = service
        .submit_proof(USER_ALICE, submit_req)
        .expect("proof submission should succeed");

    assert_eq!(res.status, VerificationStatus::Verified);
    assert!(res.attestation_id.is_some());
    assert!(res.verified_at.is_some());
    assert!(res.error_message.is_none());
}

#[tokio::test]
async fn test_08_submit_proof_attestation_id_and_verified_at_populated() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "argent".to_string(),
                provider: None,
            },
        )
        .unwrap();

    let res = service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: valid_proof_payload(),
                public_inputs: None,
            },
        )
        .unwrap();

    assert!(res.attestation_id.unwrap() != Uuid::nil());
    assert!(res.verified_at.is_some());
}

#[tokio::test]
async fn test_09_submit_proof_session_not_found() {
    let service = setup_service();
    let err = service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: Uuid::new_v4(),
                proof_blob: valid_proof_payload(),
                public_inputs: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, VerifyError::NotFound(_)));
}

#[tokio::test]
async fn test_10_submit_proof_foreign_wallet_forbidden() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    // User Bob attempts to submit proof for Alice's session
    let err = service
        .submit_proof(
            USER_BOB,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: valid_proof_payload(),
                public_inputs: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, VerifyError::Forbidden(_)));
}

#[tokio::test]
async fn test_11_submit_proof_size_limit_exceeded_bad_request() {
    let service = setup_service();
    let huge_proof = "A".repeat(MAX_PROOF_BLOB_BYTES + 1);

    let err = service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: Uuid::new_v4(),
                proof_blob: huge_proof,
                public_inputs: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, VerifyError::BadRequest(_)));
}

#[tokio::test]
async fn test_12_submit_proof_invalid_proof_blob_fails() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    let res = service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: "{\"proof\":\"invalid_signature_mock\"}".to_string(),
                public_inputs: None,
            },
        )
        .unwrap();

    assert_eq!(res.status, VerificationStatus::Failed);
    assert!(res.error_message.is_some());
    assert!(res.attestation_id.is_none());
}

#[tokio::test]
async fn test_13_submit_proof_empty_proof_blob_fails() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    let res = service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: "   ".to_string(),
                public_inputs: None,
            },
        )
        .unwrap();

    assert_eq!(res.status, VerificationStatus::Failed);
}

#[tokio::test]
async fn test_14_submit_proof_short_proof_blob_fails() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    let res = service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: "short".to_string(),
                public_inputs: None,
            },
        )
        .unwrap();

    assert_eq!(res.status, VerificationStatus::Failed);
}

#[tokio::test]
async fn test_15_submit_proof_idempotent_if_already_verified() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    let res1 = service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: valid_proof_payload(),
                public_inputs: None,
            },
        )
        .unwrap();

    let res2 = service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: valid_proof_payload(),
                public_inputs: None,
            },
        )
        .unwrap();

    assert_eq!(res1.status, VerificationStatus::Verified);
    assert_eq!(res2.status, VerificationStatus::Verified);
    assert_eq!(res1.attestation_id, res2.attestation_id);
}

#[tokio::test]
async fn test_16_get_status_verified_after_successful_submit() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: valid_proof_payload(),
                public_inputs: None,
            },
        )
        .unwrap();

    let status = service.get_status(USER_ALICE, "starkware").unwrap();
    assert_eq!(status.company_slug, "starkware");
    assert_eq!(status.status, VerificationStatus::Verified);
    assert!(status.attestation_id.is_some());
    assert!(status.verified_at.is_some());
}

#[tokio::test]
async fn test_17_get_status_pending_when_no_proof_submitted() {
    let service = setup_service();
    service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "argent".to_string(),
                provider: None,
            },
        )
        .unwrap();

    let status = service.get_status(USER_ALICE, "argent").unwrap();
    assert_eq!(status.status, VerificationStatus::Pending);
    assert!(status.attestation_id.is_none());
}

#[tokio::test]
async fn test_18_get_status_failed_after_invalid_proof_submit() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start.session_id,
                proof_blob: "corrupt_data".to_string(),
                public_inputs: None,
            },
        )
        .unwrap();

    let status = service.get_status(USER_ALICE, "starkware").unwrap();
    assert_eq!(status.status, VerificationStatus::Failed);
}

#[tokio::test]
async fn test_19_get_status_unclaimed_company_returns_pending() {
    let service = setup_service();
    let status = service
        .get_status(USER_ALICE, "never-verified-corp")
        .unwrap();
    assert_eq!(status.status, VerificationStatus::Pending);
    assert!(status.attestation_id.is_none());
}

#[tokio::test]
async fn test_20_get_status_empty_slug_bad_request() {
    let service = setup_service();
    let err = service.get_status(USER_ALICE, "   ").unwrap_err();
    assert!(matches!(err, VerifyError::BadRequest(_)));
}

#[tokio::test]
async fn test_21_verification_status_enum_serde() {
    assert_eq!(
        serde_json::to_string(&VerificationStatus::Pending).unwrap(),
        "\"pending\""
    );
    assert_eq!(
        serde_json::to_string(&VerificationStatus::Verified).unwrap(),
        "\"verified\""
    );
    assert_eq!(
        serde_json::to_string(&VerificationStatus::Failed).unwrap(),
        "\"failed\""
    );

    let parsed: VerificationStatus = serde_json::from_str("\"verified\"").unwrap();
    assert_eq!(parsed, VerificationStatus::Verified);
}

#[tokio::test]
async fn test_22_case_insensitive_wallet_matching() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    let uppercase_wallet = USER_ALICE.to_uppercase();
    let res = service.submit_proof(
        &uppercase_wallet,
        SubmitProofRequest {
            session_id: start.session_id,
            proof_blob: valid_proof_payload(),
            public_inputs: None,
        },
    );
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_23_case_insensitive_company_slug() {
    let service = setup_service();
    let start = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "StarkWare".to_string(),
                provider: None,
            },
        )
        .unwrap();
    assert_eq!(start.company_slug, "starkware");

    let status = service.get_status(USER_ALICE, "STARKWARE").unwrap();
    assert_eq!(status.company_slug, "starkware");
}

#[tokio::test]
async fn test_24_user_isolation_different_wallets_same_company() {
    let service = setup_service();
    let start_alice = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start_alice.session_id,
                proof_blob: valid_proof_payload(),
                public_inputs: None,
            },
        )
        .unwrap();

    let alice_status = service.get_status(USER_ALICE, "starkware").unwrap();
    let bob_status = service.get_status(USER_BOB, "starkware").unwrap();

    assert_eq!(alice_status.status, VerificationStatus::Verified);
    assert_eq!(bob_status.status, VerificationStatus::Pending);
}

#[tokio::test]
async fn test_25_company_isolation_same_wallet_different_companies() {
    let service = setup_service();
    let start_starkware = service
        .start_session(
            USER_ALICE,
            StartVerificationRequest {
                company_slug: "starkware".to_string(),
                provider: None,
            },
        )
        .unwrap();

    service
        .submit_proof(
            USER_ALICE,
            SubmitProofRequest {
                session_id: start_starkware.session_id,
                proof_blob: valid_proof_payload(),
                public_inputs: None,
            },
        )
        .unwrap();

    let starkware_status = service.get_status(USER_ALICE, "starkware").unwrap();
    let argent_status = service.get_status(USER_ALICE, "argent").unwrap();

    assert_eq!(starkware_status.status, VerificationStatus::Verified);
    assert_eq!(argent_status.status, VerificationStatus::Pending);
}
