use uuid::Uuid;

use backend::{
    models::claim::{AdminClaimDecisionRequest, ClaimStatus, SubmitClaimRequest},
    services::claim_service::{
        ClaimError, ClaimService, MAX_NOTE_LENGTH, MAX_PROOF_URI_LENGTH, canonical_slug,
        canonical_wallet,
    },
};

const TEST_ADMIN_WALLET: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";
const TEST_CLAIMANT_WALLET: &str =
    "0x0222222222222222222222222222222222222222222222222222222222222222";
const TEST_OTHER_WALLET: &str =
    "0x0333333333333333333333333333333333333333333333333333333333333333";

fn setup_service() -> ClaimService {
    let service = ClaimService::new();
    service.add_admin(TEST_ADMIN_WALLET);
    service
}

#[tokio::test]
async fn test_01_submit_claim_happy_path() {
    let service = setup_service();

    let req = SubmitClaimRequest {
        proof_uri: Some("https://example.com/starknet-dns-proof.txt".to_string()),
        note: Some("Official developer representative".to_string()),
    };

    let claim = service
        .submit_claim("argent-labs", TEST_CLAIMANT_WALLET, req)
        .expect("claim submission should succeed");

    assert_eq!(claim.company_slug, "argent-labs");
    assert_eq!(claim.status, ClaimStatus::Pending);
    assert_eq!(claim.decided_by, None);
    assert!(claim.proof_uri.is_some());
}

#[tokio::test]
async fn test_02_duplicate_pending_claim_returns_conflict() {
    let service = setup_service();

    let req1 = SubmitClaimRequest {
        proof_uri: Some("https://example.com/proof1".to_string()),
        note: Some("First claim".to_string()),
    };
    service
        .submit_claim("braavos", TEST_CLAIMANT_WALLET, req1)
        .unwrap();

    let req2 = SubmitClaimRequest {
        proof_uri: Some("https://example.com/proof2".to_string()),
        note: Some("Duplicate claim".to_string()),
    };
    let err = service
        .submit_claim("braavos", TEST_CLAIMANT_WALLET, req2)
        .unwrap_err();

    assert!(matches!(err, ClaimError::Conflict(_)));
}

#[tokio::test]
async fn test_03_claim_when_already_owner_returns_conflict() {
    let service = setup_service();

    service.add_company_owner("starkware", TEST_CLAIMANT_WALLET);

    let req = SubmitClaimRequest {
        proof_uri: Some("https://starkware.co".to_string()),
        note: None,
    };
    let err = service
        .submit_claim("starkware", TEST_CLAIMANT_WALLET, req)
        .unwrap_err();

    assert!(matches!(err, ClaimError::Conflict(_)));
}

#[tokio::test]
async fn test_04_proof_uri_exceeding_limit_returns_bad_request() {
    let service = setup_service();

    let long_uri = "https://example.com/".to_string() + &"a".repeat(MAX_PROOF_URI_LENGTH + 10);
    let req = SubmitClaimRequest {
        proof_uri: Some(long_uri),
        note: None,
    };
    let err = service
        .submit_claim("avnu", TEST_CLAIMANT_WALLET, req)
        .unwrap_err();

    assert!(matches!(err, ClaimError::BadRequest(_)));
}

#[tokio::test]
async fn test_05_note_exceeding_limit_returns_bad_request() {
    let service = setup_service();

    let long_note = "x".repeat(MAX_NOTE_LENGTH + 1);
    let req = SubmitClaimRequest {
        proof_uri: None,
        note: Some(long_note),
    };
    let err = service
        .submit_claim("nostra", TEST_CLAIMANT_WALLET, req)
        .unwrap_err();

    assert!(matches!(err, ClaimError::BadRequest(_)));
}

#[tokio::test]
async fn test_06_empty_company_slug_returns_bad_request() {
    let service = setup_service();

    let req = SubmitClaimRequest::default();
    let err = service
        .submit_claim("   ", TEST_CLAIMANT_WALLET, req)
        .unwrap_err();

    assert!(matches!(err, ClaimError::BadRequest(_)));
}

#[tokio::test]
async fn test_07_public_claim_status_unclaimed() {
    let service = setup_service();

    let status = service.get_company_claim_status("new-startup");
    assert_eq!(status.slug, "new-startup");
    assert!(!status.is_claimed);
    assert_eq!(status.pending_count, 0);
    assert_eq!(status.owners_count, 0);
}

#[tokio::test]
async fn test_08_public_claim_status_with_pending_claim() {
    let service = setup_service();

    service
        .submit_claim(
            "ekubo",
            TEST_CLAIMANT_WALLET,
            SubmitClaimRequest {
                proof_uri: None,
                note: Some("founder".to_string()),
            },
        )
        .unwrap();

    let status = service.get_company_claim_status("ekubo");
    assert!(!status.is_claimed);
    assert_eq!(status.pending_count, 1);
    assert_eq!(status.owners_count, 0);
}

#[tokio::test]
async fn test_09_admin_approval_happy_path() {
    let service = setup_service();

    let claim = service
        .submit_claim(
            "kakarot",
            TEST_CLAIMANT_WALLET,
            SubmitClaimRequest {
                proof_uri: Some("https://kakarot.org".to_string()),
                note: Some("Core team".to_string()),
            },
        )
        .unwrap();

    let approved = service
        .admin_decide_claim(
            "kakarot",
            claim.id,
            TEST_ADMIN_WALLET,
            AdminClaimDecisionRequest {
                action: "approve".to_string(),
                decision_note: Some("Verified domain ownership".to_string()),
            },
        )
        .expect("admin approval should succeed");

    assert_eq!(approved.status, ClaimStatus::Approved);
    assert_eq!(
        approved.decision_note,
        Some("Verified domain ownership".to_string())
    );
    assert!(approved.decided_by.is_some());

    // Verify wallet is now linked as company owner
    assert!(service.is_company_owner("kakarot", TEST_CLAIMANT_WALLET));

    // Verify public claim status reflects claimed state
    let status = service.get_company_claim_status("kakarot");
    assert!(status.is_claimed);
    assert_eq!(status.owners_count, 1);
    assert_eq!(status.pending_count, 0);
}

#[tokio::test]
async fn test_10_non_admin_approval_returns_forbidden() {
    let service = setup_service();

    let claim = service
        .submit_claim("fibrous", TEST_CLAIMANT_WALLET, SubmitClaimRequest::default())
        .unwrap();

    let err = service
        .admin_decide_claim(
            "fibrous",
            claim.id,
            TEST_OTHER_WALLET,
            AdminClaimDecisionRequest {
                action: "approve".to_string(),
                decision_note: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, ClaimError::Forbidden(_)));
}

#[tokio::test]
async fn test_11_admin_rejection_happy_path() {
    let service = setup_service();

    let claim = service
        .submit_claim(
            "zkx",
            TEST_CLAIMANT_WALLET,
            SubmitClaimRequest {
                proof_uri: None,
                note: Some("Anonymous applicant".to_string()),
            },
        )
        .unwrap();

    let rejected = service
        .admin_decide_claim(
            "zkx",
            claim.id,
            TEST_ADMIN_WALLET,
            AdminClaimDecisionRequest {
                action: "reject".to_string(),
                decision_note: Some("Insufficient proof provided".to_string()),
            },
        )
        .expect("rejection should succeed");

    assert_eq!(rejected.status, ClaimStatus::Rejected);
    assert_eq!(
        rejected.decision_note,
        Some("Insufficient proof provided".to_string())
    );

    // Verify wallet is NOT linked as owner
    assert!(!service.is_company_owner("zkx", TEST_CLAIMANT_WALLET));
}

#[tokio::test]
async fn test_12_invalid_decision_action_returns_bad_request() {
    let service = setup_service();

    let claim = service
        .submit_claim("carmine", TEST_CLAIMANT_WALLET, SubmitClaimRequest::default())
        .unwrap();

    let err = service
        .admin_decide_claim(
            "carmine",
            claim.id,
            TEST_ADMIN_WALLET,
            AdminClaimDecisionRequest {
                action: "escalate".to_string(),
                decision_note: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, ClaimError::BadRequest(_)));
}

#[tokio::test]
async fn test_13_claim_not_found_returns_not_found() {
    let service = setup_service();

    let err = service
        .admin_decide_claim(
            "vesu",
            Uuid::new_v4(),
            TEST_ADMIN_WALLET,
            AdminClaimDecisionRequest {
                action: "approve".to_string(),
                decision_note: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, ClaimError::NotFound(_)));
}

#[tokio::test]
async fn test_14_claim_wrong_company_slug_returns_not_found() {
    let service = setup_service();

    let claim = service
        .submit_claim(
            "paradex",
            TEST_CLAIMANT_WALLET,
            SubmitClaimRequest::default(),
        )
        .unwrap();

    let err = service
        .admin_decide_claim(
            "other-company",
            claim.id,
            TEST_ADMIN_WALLET,
            AdminClaimDecisionRequest {
                action: "approve".to_string(),
                decision_note: None,
            },
        )
        .unwrap_err();

    assert!(matches!(err, ClaimError::NotFound(_)));
}

#[tokio::test]
async fn test_15_multiple_company_owners_supported() {
    let service = setup_service();

    service.add_company_owner("nethermind", TEST_CLAIMANT_WALLET);
    service.add_company_owner("nethermind", TEST_OTHER_WALLET);

    assert!(service.is_company_owner("nethermind", TEST_CLAIMANT_WALLET));
    assert!(service.is_company_owner("nethermind", TEST_OTHER_WALLET));

    let status = service.get_company_claim_status("nethermind");
    assert_eq!(status.owners_count, 2);
}

#[tokio::test]
async fn test_16_company_slug_case_insensitivity() {
    let service = setup_service();

    service
        .submit_claim(
            "PrAgMa",
            TEST_CLAIMANT_WALLET,
            SubmitClaimRequest {
                proof_uri: None,
                note: None,
            },
        )
        .unwrap();

    let status = service.get_company_claim_status("pragma");
    assert_eq!(status.pending_count, 1);
}

#[tokio::test]
async fn test_17_wallet_normalization_and_comparison() {
    let service = setup_service();

    let mixed_case_wallet = "0x0AABBCCDDEEFF00112233445566778899AABBCCDDEEFF0011223344556677889";
    service.add_admin(mixed_case_wallet);

    let lower_wallet = mixed_case_wallet.to_lowercase();
    assert!(service.is_admin(&lower_wallet));
}

#[tokio::test]
async fn test_18_get_claim_by_id_retrieval() {
    let service = setup_service();

    let claim = service
        .submit_claim(
            "focus-tree",
            TEST_CLAIMANT_WALLET,
            SubmitClaimRequest {
                proof_uri: Some("https://focustree.app".to_string()),
                note: Some("lead".to_string()),
            },
        )
        .unwrap();

    let retrieved = service
        .get_claim(claim.id)
        .expect("claim should be retrieved");
    assert_eq!(retrieved.id, claim.id);
    assert_eq!(retrieved.company_slug, "focus-tree");
}

#[tokio::test]
async fn test_19_canonical_slug_and_wallet_helpers() {
    assert_eq!(canonical_slug("  StarkNet-Hub  "), "starknet-hub");
    let wallet = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let norm = canonical_wallet(wallet);
    assert!(norm.starts_with("0x"));
    assert_eq!(norm, "0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");
}

#[tokio::test]
async fn test_20_rejection_allows_new_claim_submission() {
    let service = setup_service();

    let claim = service
        .submit_claim(
            "avnu",
            TEST_CLAIMANT_WALLET,
            SubmitClaimRequest {
                proof_uri: None,
                note: Some("draft".to_string()),
            },
        )
        .unwrap();

    service
        .admin_decide_claim(
            "avnu",
            claim.id,
            TEST_ADMIN_WALLET,
            AdminClaimDecisionRequest {
                action: "reject".to_string(),
                decision_note: Some("resubmit".to_string()),
            },
        )
        .unwrap();

    // After rejection, user should be able to submit a new claim
    let new_claim = service
        .submit_claim(
            "avnu",
            TEST_CLAIMANT_WALLET,
            SubmitClaimRequest {
                proof_uri: Some("https://new-proof".to_string()),
                note: Some("second attempt".to_string()),
            },
        )
        .expect("new claim after rejection should succeed");

    assert_eq!(new_claim.status, ClaimStatus::Pending);
}
