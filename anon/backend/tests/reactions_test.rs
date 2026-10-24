use axum::http::{StatusCode, header};
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::models::reaction::{
    CreateReactionDto, ReactionResponse, ReactionSummary, ReactionTarget, ReactionType,
};
use backend::services::reaction_service::{
    REACTION_STORE, ReactionStore, canonical_wallet,
};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-reactions") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-reactions") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

// =========================================================================
// D1: TYPE VALIDATION, PARSING & BOUNDARY CONDITIONS (20 tests)
// =========================================================================

#[test]
fn test_d1_01_parse_valid_reaction_type_upvote() {
    assert_eq!(ReactionType::from_str("upvote").unwrap(), ReactionType::Upvote);
}

#[test]
fn test_d1_02_parse_valid_reaction_type_insightful() {
    assert_eq!(ReactionType::from_str("insightful").unwrap(), ReactionType::Insightful);
}

#[test]
fn test_d1_03_parse_valid_reaction_type_thanks() {
    assert_eq!(ReactionType::from_str("thanks").unwrap(), ReactionType::Thanks);
}

#[test]
fn test_d1_04_parse_case_insensitive_reaction_types() {
    assert_eq!(ReactionType::from_str("UPVOTE").unwrap(), ReactionType::Upvote);
    assert_eq!(ReactionType::from_str("InSigHtFul").unwrap(), ReactionType::Insightful);
    assert_eq!(ReactionType::from_str("  THANKS  ").unwrap(), ReactionType::Thanks);
}

#[test]
fn test_d1_05_parse_invalid_reaction_type_fails() {
    assert!(ReactionType::from_str("dislike").is_err());
    assert!(ReactionType::from_str("heart").is_err());
    assert!(ReactionType::from_str("12345").is_err());
}

#[test]
fn test_d1_06_parse_empty_reaction_type_fails() {
    assert!(ReactionType::from_str("").is_err());
    assert!(ReactionType::from_str("   ").is_err());
}

#[test]
fn test_d1_07_reaction_type_display_and_as_str() {
    assert_eq!(ReactionType::Upvote.as_str(), "upvote");
    assert_eq!(ReactionType::Insightful.as_str(), "insightful");
    assert_eq!(ReactionType::Thanks.as_str(), "thanks");
    assert_eq!(format!("{}", ReactionType::Upvote), "upvote");
}

#[test]
fn test_d1_08_reaction_type_serde_roundtrip() {
    let serialized = serde_json::to_string(&ReactionType::Insightful).unwrap();
    assert_eq!(serialized, "\"insightful\"");
    let deserialized: ReactionType = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, ReactionType::Insightful);
}

#[test]
fn test_d1_09_parse_valid_target_type_review() {
    assert_eq!(ReactionTarget::from_str("review").unwrap(), ReactionTarget::Review);
    assert_eq!(ReactionTarget::from_str("reviews").unwrap(), ReactionTarget::Review);
    assert_eq!(ReactionTarget::from_str("REVIEW").unwrap(), ReactionTarget::Review);
}

#[test]
fn test_d1_10_parse_valid_target_type_comment() {
    assert_eq!(ReactionTarget::from_str("comment").unwrap(), ReactionTarget::Comment);
    assert_eq!(ReactionTarget::from_str("comments").unwrap(), ReactionTarget::Comment);
    assert_eq!(ReactionTarget::from_str("COMMENT").unwrap(), ReactionTarget::Comment);
}

#[test]
fn test_d1_11_parse_invalid_target_type_fails() {
    assert!(ReactionTarget::from_str("post").is_err());
    assert!(ReactionTarget::from_str("user").is_err());
    assert!(ReactionTarget::from_str("channel").is_err());
}

#[test]
fn test_d1_12_parse_empty_target_type_fails() {
    assert!(ReactionTarget::from_str("").is_err());
}

#[test]
fn test_d1_13_target_type_display_and_as_str() {
    assert_eq!(ReactionTarget::Review.as_str(), "review");
    assert_eq!(ReactionTarget::Comment.as_str(), "comment");
    assert_eq!(format!("{}", ReactionTarget::Review), "review");
}

#[test]
fn test_d1_14_target_type_serde_roundtrip() {
    let serialized = serde_json::to_string(&ReactionTarget::Comment).unwrap();
    assert_eq!(serialized, "\"comment\"");
    let deserialized: ReactionTarget = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, ReactionTarget::Comment);
}

#[test]
fn test_d1_15_reaction_summary_default_is_zero() {
    let s = ReactionSummary::default();
    assert_eq!(s.upvotes, 0);
    assert_eq!(s.insightful, 0);
    assert_eq!(s.thanks, 0);
    assert_eq!(s.total, 0);
    assert_eq!(s.user_reaction, None);
}

#[test]
fn test_d1_16_canonical_wallet_normalization() {
    assert_eq!(canonical_wallet("0x123AbC"), "0x123abc");
    assert_eq!(canonical_wallet("123abc"), "0x123abc");
    assert_eq!(canonical_wallet("  0X456DEF  "), "0x456def");
}

#[tokio::test]
async fn test_d1_17_http_post_invalid_reaction_type_returns_400() {
    let server = setup_test_app();
    let token = create_auth_token("0x101");
    let res = server
        .post("/reactions/review/target-d1-17")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "invalid_type".to_string(),
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d1_18_http_post_invalid_target_type_returns_400() {
    let server = setup_test_app();
    let token = create_auth_token("0x101");
    let res = server
        .post("/reactions/invalid_target/target-d1-18")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d1_19_http_delete_invalid_target_type_returns_400() {
    let server = setup_test_app();
    let token = create_auth_token("0x101");
    let res = server
        .delete("/reactions/unknown_target/target-d1-19")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d1_20_http_get_invalid_target_type_returns_400() {
    let server = setup_test_app();
    let res = server.get("/reactions/unknown_target/target-d1-20").await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

// =========================================================================
// D2: IDEMPOTENCE, SWITCHING & DELETION SEMANTICS (20 tests)
// =========================================================================

#[test]
fn test_d2_01_first_reaction_returns_recorded() {
    let store = ReactionStore::new();
    let res = store
        .record_reaction("0x201", ReactionTarget::Review, "target-d2-01", ReactionType::Upvote)
        .unwrap();
    assert_eq!(res.status, "recorded");
}

#[test]
fn test_d2_02_first_upvote_increments_upvotes_to_one() {
    let store = ReactionStore::new();
    let res = store
        .record_reaction("0x202", ReactionTarget::Review, "target-d2-02", ReactionType::Upvote)
        .unwrap();
    assert_eq!(res.summary.upvotes, 1);
    assert_eq!(res.summary.total, 1);
    assert_eq!(res.summary.user_reaction, Some(ReactionType::Upvote));
}

#[test]
fn test_d2_03_repeated_identical_reaction_returns_unchanged() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x203", ReactionTarget::Review, "target-d2-03", ReactionType::Upvote)
        .unwrap();
    let res2 = store
        .record_reaction("0x203", ReactionTarget::Review, "target-d2-03", ReactionType::Upvote)
        .unwrap();
    assert_eq!(res2.status, "unchanged");
}

#[test]
fn test_d2_04_repeated_identical_reaction_keeps_count_at_one() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x204", ReactionTarget::Review, "target-d2-04", ReactionType::Insightful)
        .unwrap();
    let res2 = store
        .record_reaction("0x204", ReactionTarget::Review, "target-d2-04", ReactionType::Insightful)
        .unwrap();
    assert_eq!(res2.summary.insightful, 1);
    assert_eq!(res2.summary.total, 1);
}

#[test]
fn test_d2_05_switch_from_upvote_to_insightful_returns_switched() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x205", ReactionTarget::Review, "target-d2-05", ReactionType::Upvote)
        .unwrap();
    let res2 = store
        .record_reaction("0x205", ReactionTarget::Review, "target-d2-05", ReactionType::Insightful)
        .unwrap();
    assert_eq!(res2.status, "switched");
}

#[test]
fn test_d2_06_switch_from_upvote_to_insightful_decrements_upvote_and_increments_insightful() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x206", ReactionTarget::Review, "target-d2-06", ReactionType::Upvote)
        .unwrap();
    let res2 = store
        .record_reaction("0x206", ReactionTarget::Review, "target-d2-06", ReactionType::Insightful)
        .unwrap();
    assert_eq!(res2.summary.upvotes, 0);
    assert_eq!(res2.summary.insightful, 1);
    assert_eq!(res2.summary.total, 1);
    assert_eq!(res2.summary.user_reaction, Some(ReactionType::Insightful));
}

#[test]
fn test_d2_07_switch_from_insightful_to_thanks_returns_switched() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x207", ReactionTarget::Comment, "target-d2-07", ReactionType::Insightful)
        .unwrap();
    let res2 = store
        .record_reaction("0x207", ReactionTarget::Comment, "target-d2-07", ReactionType::Thanks)
        .unwrap();
    assert_eq!(res2.status, "switched");
}

#[test]
fn test_d2_08_switch_from_insightful_to_thanks_updates_counts_atomically() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x208", ReactionTarget::Comment, "target-d2-08", ReactionType::Insightful)
        .unwrap();
    let res2 = store
        .record_reaction("0x208", ReactionTarget::Comment, "target-d2-08", ReactionType::Thanks)
        .unwrap();
    assert_eq!(res2.summary.insightful, 0);
    assert_eq!(res2.summary.thanks, 1);
    assert_eq!(res2.summary.total, 1);
    assert_eq!(res2.summary.user_reaction, Some(ReactionType::Thanks));
}

#[test]
fn test_d2_09_switch_from_thanks_back_to_upvote() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x209", ReactionTarget::Comment, "target-d2-09", ReactionType::Thanks)
        .unwrap();
    let res = store
        .record_reaction("0x209", ReactionTarget::Comment, "target-d2-09", ReactionType::Upvote)
        .unwrap();
    assert_eq!(res.status, "switched");
    assert_eq!(res.summary.thanks, 0);
    assert_eq!(res.summary.upvotes, 1);
}

#[test]
fn test_d2_10_repeated_thanks_is_idempotent() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x210", ReactionTarget::Review, "target-d2-10", ReactionType::Thanks)
        .unwrap();
    let res = store
        .record_reaction("0x210", ReactionTarget::Review, "target-d2-10", ReactionType::Thanks)
        .unwrap();
    assert_eq!(res.status, "unchanged");
    assert_eq!(res.summary.thanks, 1);
}

#[test]
fn test_d2_11_multiple_users_reacting_with_same_type_increments_count() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x211a", ReactionTarget::Review, "target-d2-11", ReactionType::Upvote)
        .unwrap();
    let res2 = store
        .record_reaction("0x211b", ReactionTarget::Review, "target-d2-11", ReactionType::Upvote)
        .unwrap();
    assert_eq!(res2.summary.upvotes, 2);
    assert_eq!(res2.summary.total, 2);
}

#[test]
fn test_d2_12_multiple_users_reacting_with_different_types() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x212a", ReactionTarget::Review, "target-d2-12", ReactionType::Upvote)
        .unwrap();
    store
        .record_reaction("0x212b", ReactionTarget::Review, "target-d2-12", ReactionType::Insightful)
        .unwrap();
    let res3 = store
        .record_reaction("0x212c", ReactionTarget::Review, "target-d2-12", ReactionType::Thanks)
        .unwrap();
    assert_eq!(res3.summary.upvotes, 1);
    assert_eq!(res3.summary.insightful, 1);
    assert_eq!(res3.summary.thanks, 1);
    assert_eq!(res3.summary.total, 3);
}

#[test]
fn test_d2_13_user_a_switch_does_not_affect_user_b_reaction() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x213a", ReactionTarget::Review, "target-d2-13", ReactionType::Upvote)
        .unwrap();
    store
        .record_reaction("0x213b", ReactionTarget::Review, "target-d2-13", ReactionType::Insightful)
        .unwrap();
    let res = store
        .record_reaction("0x213a", ReactionTarget::Review, "target-d2-13", ReactionType::Thanks)
        .unwrap();
    assert_eq!(res.summary.upvotes, 0);
    assert_eq!(res.summary.insightful, 1);
    assert_eq!(res.summary.thanks, 1);
    assert_eq!(res.summary.total, 2);
}

#[test]
fn test_d2_14_delete_existing_reaction_returns_removed() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x214", ReactionTarget::Review, "target-d2-14", ReactionType::Upvote)
        .unwrap();
    let del = store
        .remove_reaction("0x214", ReactionTarget::Review, "target-d2-14")
        .unwrap();
    assert_eq!(del.status, "removed");
}

#[test]
fn test_d2_15_delete_existing_reaction_decrements_count_to_zero() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x215", ReactionTarget::Review, "target-d2-15", ReactionType::Upvote)
        .unwrap();
    let del = store
        .remove_reaction("0x215", ReactionTarget::Review, "target-d2-15")
        .unwrap();
    assert_eq!(del.summary.upvotes, 0);
    assert_eq!(del.summary.total, 0);
    assert_eq!(del.summary.user_reaction, None);
}

#[test]
fn test_d2_16_repeated_delete_returns_unchanged_idempotently() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x216", ReactionTarget::Review, "target-d2-16", ReactionType::Upvote)
        .unwrap();
    store
        .remove_reaction("0x216", ReactionTarget::Review, "target-d2-16")
        .unwrap();
    let del2 = store
        .remove_reaction("0x216", ReactionTarget::Review, "target-d2-16")
        .unwrap();
    assert_eq!(del2.status, "unchanged");
}

#[test]
fn test_d2_17_repeated_delete_never_causes_negative_counters() {
    let store = ReactionStore::new();
    for _ in 0..5 {
        let del = store
            .remove_reaction("0x217", ReactionTarget::Review, "target-d2-17")
            .unwrap();
        assert_eq!(del.summary.total, 0);
    }
}

#[test]
fn test_d2_18_react_after_delete_works_as_fresh_record() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x218", ReactionTarget::Comment, "target-d2-18", ReactionType::Upvote)
        .unwrap();
    store
        .remove_reaction("0x218", ReactionTarget::Comment, "target-d2-18")
        .unwrap();
    let fresh = store
        .record_reaction("0x218", ReactionTarget::Comment, "target-d2-18", ReactionType::Insightful)
        .unwrap();
    assert_eq!(fresh.status, "recorded");
    assert_eq!(fresh.summary.insightful, 1);
    assert_eq!(fresh.summary.user_reaction, Some(ReactionType::Insightful));
}

#[tokio::test]
async fn test_d2_19_http_post_idempotent_flow() {
    let server = setup_test_app();
    let token = create_auth_token("0x219");
    let target_id = "target-d2-19";

    let res1 = server
        .post(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    assert_eq!(res1.status_code(), StatusCode::OK);
    let body1: ReactionResponse = res1.json();
    assert_eq!(body1.status, "recorded");
    assert_eq!(body1.summary.upvotes, 1);

    // Repeat identical request
    let res2 = server
        .post(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    assert_eq!(res2.status_code(), StatusCode::OK);
    let body2: ReactionResponse = res2.json();
    assert_eq!(body2.status, "unchanged");
    assert_eq!(body2.summary.upvotes, 1);
}

#[tokio::test]
async fn test_d2_20_http_delete_idempotent_flow() {
    let server = setup_test_app();
    let token = create_auth_token("0x220");
    let target_id = "target-d2-20";

    server
        .post(&format!("/reactions/comment/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "thanks".to_string(),
        })
        .await;

    let del1 = server
        .delete(&format!("/reactions/comment/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(del1.status_code(), StatusCode::OK);
    let body_del1: ReactionResponse = del1.json();
    assert_eq!(body_del1.status, "removed");

    let del2 = server
        .delete(&format!("/reactions/comment/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(del2.status_code(), StatusCode::OK);
    let body_del2: ReactionResponse = del2.json();
    assert_eq!(body_del2.status, "unchanged");
}

// =========================================================================
// D3: AGGREGATED COUNTERS & TARGET ISOLATION (20 tests)
// =========================================================================

#[test]
fn test_d3_01_target_isolation_reviews_with_different_ids() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x301", ReactionTarget::Review, "rev-A", ReactionType::Upvote)
        .unwrap();
    let s_a = store.get_summary(None, ReactionTarget::Review, "rev-A");
    let s_b = store.get_summary(None, ReactionTarget::Review, "rev-B");
    assert_eq!(s_a.upvotes, 1);
    assert_eq!(s_b.upvotes, 0);
}

#[test]
fn test_d3_02_target_isolation_comments_with_different_ids() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x302", ReactionTarget::Comment, "com-1", ReactionType::Thanks)
        .unwrap();
    let s1 = store.get_summary(None, ReactionTarget::Comment, "com-1");
    let s2 = store.get_summary(None, ReactionTarget::Comment, "com-2");
    assert_eq!(s1.thanks, 1);
    assert_eq!(s2.thanks, 0);
}

#[test]
fn test_d3_03_type_isolation_review_vs_comment_with_same_id() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x303", ReactionTarget::Review, "shared-id", ReactionType::Upvote)
        .unwrap();
    let s_rev = store.get_summary(None, ReactionTarget::Review, "shared-id");
    let s_com = store.get_summary(None, ReactionTarget::Comment, "shared-id");
    assert_eq!(s_rev.upvotes, 1);
    assert_eq!(s_com.upvotes, 0);
}

#[test]
fn test_d3_04_summary_total_always_equals_sum_of_types() {
    let store = ReactionStore::new();
    store.record_reaction("0x304a", ReactionTarget::Review, "t4", ReactionType::Upvote).unwrap();
    store.record_reaction("0x304b", ReactionTarget::Review, "t4", ReactionType::Insightful).unwrap();
    store.record_reaction("0x304c", ReactionTarget::Review, "t4", ReactionType::Thanks).unwrap();
    let s = store.get_summary(None, ReactionTarget::Review, "t4");
    assert_eq!(s.total, s.upvotes + s.insightful + s.thanks);
    assert_eq!(s.total, 3);
}

#[test]
fn test_d3_05_summary_user_reaction_reflects_current_user() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x305", ReactionTarget::Review, "t5", ReactionType::Insightful)
        .unwrap();
    let s = store.get_summary(Some("0x305"), ReactionTarget::Review, "t5");
    assert_eq!(s.user_reaction, Some(ReactionType::Insightful));
}

#[test]
fn test_d3_06_summary_user_reaction_is_none_for_other_users() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x306a", ReactionTarget::Review, "t6", ReactionType::Insightful)
        .unwrap();
    let s = store.get_summary(Some("0x306b"), ReactionTarget::Review, "t6");
    assert_eq!(s.user_reaction, None);
}

#[test]
fn test_d3_07_summary_without_user_returns_counts_with_user_reaction_none() {
    let store = ReactionStore::new();
    store
        .record_reaction("0x307", ReactionTarget::Review, "t7", ReactionType::Upvote)
        .unwrap();
    let s = store.get_summary(None, ReactionTarget::Review, "t7");
    assert_eq!(s.upvotes, 1);
    assert_eq!(s.user_reaction, None);
}

#[test]
fn test_d3_08_ten_users_upvoting_gives_10_upvotes_total_10() {
    let store = ReactionStore::new();
    for i in 0..10 {
        store
            .record_reaction(
                &format!("0x308_{}", i),
                ReactionTarget::Review,
                "t8",
                ReactionType::Upvote,
            )
            .unwrap();
    }
    let s = store.get_summary(None, ReactionTarget::Review, "t8");
    assert_eq!(s.upvotes, 10);
    assert_eq!(s.total, 10);
}

#[test]
fn test_d3_09_mixed_reactions_aggregate_accurately() {
    let store = ReactionStore::new();
    // 5 upvotes, 3 insightful, 2 thanks
    for i in 0..5 {
        store
            .record_reaction(&format!("0x309_u_{}", i), ReactionTarget::Review, "t9", ReactionType::Upvote)
            .unwrap();
    }
    for i in 0..3 {
        store
            .record_reaction(&format!("0x309_i_{}", i), ReactionTarget::Review, "t9", ReactionType::Insightful)
            .unwrap();
    }
    for i in 0..2 {
        store
            .record_reaction(&format!("0x309_t_{}", i), ReactionTarget::Review, "t9", ReactionType::Thanks)
            .unwrap();
    }
    let s = store.get_summary(None, ReactionTarget::Review, "t9");
    assert_eq!(s.upvotes, 5);
    assert_eq!(s.insightful, 3);
    assert_eq!(s.thanks, 2);
    assert_eq!(s.total, 10);
}

#[test]
fn test_d3_10_clearing_target_resets_counters() {
    let store = ReactionStore::new();
    store.record_reaction("0x310", ReactionTarget::Review, "t10", ReactionType::Upvote).unwrap();
    store.clear();
    let s = store.get_summary(None, ReactionTarget::Review, "t10");
    assert_eq!(s.total, 0);
}

#[tokio::test]
async fn test_d3_11_http_get_summary_unauthenticated() {
    let server = setup_test_app();
    let token = create_auth_token("0x311");
    let target_id = "target-d3-11";

    server
        .post(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;

    let res = server.get(&format!("/reactions/review/{}", target_id)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let s: ReactionSummary = res.json();
    assert_eq!(s.upvotes, 1);
    assert_eq!(s.user_reaction, None);
}

#[tokio::test]
async fn test_d3_12_http_get_summary_authenticated_includes_user_reaction() {
    let server = setup_test_app();
    let wallet = "0x312";
    let token = create_auth_token(wallet);
    let target_id = "target-d3-12";

    server
        .post(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "insightful".to_string(),
        })
        .await;

    let res = server
        .get(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let s: ReactionSummary = res.json();
    assert_eq!(s.insightful, 1);
    assert_eq!(s.user_reaction, Some(ReactionType::Insightful));
}

#[tokio::test]
async fn test_d3_13_http_get_summary_after_switch_shows_updated_reaction() {
    let server = setup_test_app();
    let wallet = "0x313";
    let token = create_auth_token(wallet);
    let target_id = "target-d3-13";

    server
        .post(&format!("/reactions/comment/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;

    server
        .post(&format!("/reactions/comment/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "thanks".to_string(),
        })
        .await;

    let res = server
        .get(&format!("/reactions/comment/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    let s: ReactionSummary = res.json();
    assert_eq!(s.upvotes, 0);
    assert_eq!(s.thanks, 1);
    assert_eq!(s.user_reaction, Some(ReactionType::Thanks));
}

#[tokio::test]
async fn test_d3_14_http_get_summary_after_delete_shows_none_reaction() {
    let server = setup_test_app();
    let wallet = "0x314";
    let token = create_auth_token(wallet);
    let target_id = "target-d3-14";

    server
        .post(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;

    server
        .delete(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    let res = server
        .get(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    let s: ReactionSummary = res.json();
    assert_eq!(s.total, 0);
    assert_eq!(s.user_reaction, None);
}

#[test]
fn test_d3_15_summary_serialization_fields_match_schema() {
    let s = ReactionSummary {
        upvotes: 4,
        insightful: 2,
        thanks: 1,
        total: 7,
        user_reaction: Some(ReactionType::Upvote),
    };
    let json_str = serde_json::to_string(&s).unwrap();
    assert!(json_str.contains("\"upvotes\":4"));
    assert!(json_str.contains("\"insightful\":2"));
    assert!(json_str.contains("\"thanks\":1"));
    assert!(json_str.contains("\"total\":7"));
    assert!(json_str.contains("\"user_reaction\":\"upvote\""));
}

#[test]
fn test_d3_16_multiple_comments_distinct_counts() {
    let store = ReactionStore::new();
    store.record_reaction("0x316", ReactionTarget::Comment, "c1", ReactionType::Upvote).unwrap();
    store.record_reaction("0x316", ReactionTarget::Comment, "c2", ReactionType::Thanks).unwrap();
    let s1 = store.get_summary(None, ReactionTarget::Comment, "c1");
    let s2 = store.get_summary(None, ReactionTarget::Comment, "c2");
    assert_eq!(s1.upvotes, 1);
    assert_eq!(s1.thanks, 0);
    assert_eq!(s2.upvotes, 0);
    assert_eq!(s2.thanks, 1);
}

#[test]
fn test_d3_17_cross_target_deletion_isolation() {
    let store = ReactionStore::new();
    store.record_reaction("0x317", ReactionTarget::Review, "target", ReactionType::Upvote).unwrap();
    store.record_reaction("0x317", ReactionTarget::Comment, "target", ReactionType::Upvote).unwrap();
    store.remove_reaction("0x317", ReactionTarget::Review, "target").unwrap();
    let s_rev = store.get_summary(None, ReactionTarget::Review, "target");
    let s_com = store.get_summary(None, ReactionTarget::Comment, "target");
    assert_eq!(s_rev.total, 0);
    assert_eq!(s_com.total, 1);
}

#[test]
fn test_d3_18_large_aggregate_counter_accuracy() {
    let store = ReactionStore::new();
    for i in 0..30 {
        store.record_reaction(&format!("0x318_{}", i), ReactionTarget::Review, "big", ReactionType::Upvote).unwrap();
    }
    let s = store.get_summary(None, ReactionTarget::Review, "big");
    assert_eq!(s.upvotes, 30);
    assert_eq!(s.total, 30);
}

#[test]
fn test_d3_19_empty_target_summary_has_all_zeroes() {
    let store = ReactionStore::new();
    let s = store.get_summary(None, ReactionTarget::Review, "nonexistent");
    assert_eq!(s.total, 0);
    assert_eq!(s.upvotes, 0);
    assert_eq!(s.insightful, 0);
    assert_eq!(s.thanks, 0);
}

#[test]
fn test_d3_20_store_clear_removes_all_targets() {
    let store = ReactionStore::new();
    store.record_reaction("0x320a", ReactionTarget::Review, "r1", ReactionType::Upvote).unwrap();
    store.record_reaction("0x320b", ReactionTarget::Comment, "c1", ReactionType::Thanks).unwrap();
    store.clear();
    assert_eq!(store.get_summary(None, ReactionTarget::Review, "r1").total, 0);
    assert_eq!(store.get_summary(None, ReactionTarget::Comment, "c1").total, 0);
}

// =========================================================================
// D4: ANTI-ABUSE, SELF-REACTION & AUTHORIZATION (20 tests)
// =========================================================================

#[test]
fn test_d4_01_self_reaction_to_own_review_fails_forbidden() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Review, "rev-401", "0x401");
    let res = store.record_reaction("0x401", ReactionTarget::Review, "rev-401", ReactionType::Upvote);
    assert!(matches!(res, Err(backend::libs::error::ApiError::Forbidden(_))));
}

#[test]
fn test_d4_02_self_reaction_to_own_comment_fails_forbidden() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Comment, "com-402", "0x402");
    let res = store.record_reaction("0x402", ReactionTarget::Comment, "com-402", ReactionType::Thanks);
    assert!(matches!(res, Err(backend::libs::error::ApiError::Forbidden(_))));
}

#[test]
fn test_d4_03_non_author_can_react_to_review() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Review, "rev-403", "0xauthor403");
    let res = store.record_reaction("0xvoter403", ReactionTarget::Review, "rev-403", ReactionType::Upvote);
    assert!(res.is_ok());
}

#[test]
fn test_d4_04_non_author_can_react_to_comment() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Comment, "com-404", "0xauthor404");
    let res = store.record_reaction("0xvoter404", ReactionTarget::Comment, "com-404", ReactionType::Insightful);
    assert!(res.is_ok());
}

#[test]
fn test_d4_05_author_wallet_case_insensitivity_in_self_reaction_check() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Review, "rev-405", "0xAbCdEf");
    let res = store.record_reaction("0xabcdef", ReactionTarget::Review, "rev-405", ReactionType::Upvote);
    assert!(matches!(res, Err(backend::libs::error::ApiError::Forbidden(_))));
}

#[test]
fn test_d4_06_author_without_0x_prefix_normalized_in_check() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Review, "rev-406", "12345");
    let res = store.record_reaction("0x12345", ReactionTarget::Review, "rev-406", ReactionType::Upvote);
    assert!(matches!(res, Err(backend::libs::error::ApiError::Forbidden(_))));
}

#[tokio::test]
async fn test_d4_07_http_post_to_own_content_returns_403_forbidden() {
    let server = setup_test_app();
    let author_wallet = "0x407";
    let token = create_auth_token(author_wallet);
    let target_id = "target-d4-07";

    REACTION_STORE.set_target_author(ReactionTarget::Review, target_id, author_wallet);

    let res = server
        .post(&format!("/reactions/review/{}", target_id))
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_d4_08_http_post_without_auth_header_returns_401() {
    let server = setup_test_app();
    let res = server
        .post("/reactions/review/target-d4-08")
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_09_http_post_with_invalid_bearer_token_returns_401() {
    let server = setup_test_app();
    let res = server
        .post("/reactions/review/target-d4-09")
        .add_header(header::AUTHORIZATION, "Bearer invalid-jwt-token-xyz")
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_10_http_post_with_malformed_token_returns_401() {
    let server = setup_test_app();
    let res = server
        .post("/reactions/review/target-d4-10")
        .add_header(header::AUTHORIZATION, "Basic dXNlcjpwYXNz")
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_11_http_delete_without_auth_header_returns_401() {
    let server = setup_test_app();
    let res = server.delete("/reactions/review/target-d4-11").await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_12_http_delete_with_invalid_bearer_token_returns_401() {
    let server = setup_test_app();
    let res = server
        .delete("/reactions/review/target-d4-12")
        .add_header(header::AUTHORIZATION, "Bearer bogus")
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_d4_13_rate_limiting_exceeding_threshold_triggers_429() {
    let store = ReactionStore::new();
    store.set_rate_limit(3, Duration::from_secs(60));
    let user = "0x413";

    assert!(store.record_reaction(user, ReactionTarget::Review, "t1", ReactionType::Upvote).is_ok());
    assert!(store.record_reaction(user, ReactionTarget::Review, "t2", ReactionType::Upvote).is_ok());
    assert!(store.record_reaction(user, ReactionTarget::Review, "t3", ReactionType::Upvote).is_ok());
    // 4th request exceeds rate limit
    let res4 = store.record_reaction(user, ReactionTarget::Review, "t4", ReactionType::Upvote);
    assert!(matches!(res4, Err(backend::libs::error::ApiError::TooManyRequests(_))));
}

#[test]
fn test_d4_14_rate_limiting_under_threshold_succeeds() {
    let store = ReactionStore::new();
    store.set_rate_limit(5, Duration::from_secs(60));
    let user = "0x414";
    for i in 0..5 {
        assert!(store.record_reaction(user, ReactionTarget::Review, &format!("t_{}", i), ReactionType::Upvote).is_ok());
    }
}

#[test]
fn test_d4_15_rate_limiting_is_per_user_wallet() {
    let store = ReactionStore::new();
    store.set_rate_limit(1, Duration::from_secs(60));
    assert!(store.record_reaction("0x415a", ReactionTarget::Review, "t1", ReactionType::Upvote).is_ok());
    // Different user is not blocked
    assert!(store.record_reaction("0x415b", ReactionTarget::Review, "t1", ReactionType::Upvote).is_ok());
}

#[test]
fn test_d4_16_rate_limiting_reset_allows_requests_again() {
    let store = ReactionStore::new();
    store.set_rate_limit(1, Duration::from_secs(60));
    let user = "0x416";
    assert!(store.record_reaction(user, ReactionTarget::Review, "t1", ReactionType::Upvote).is_ok());
    assert!(store.record_reaction(user, ReactionTarget::Review, "t2", ReactionType::Upvote).is_err());
    store.reset_rate_limits();
    assert!(store.record_reaction(user, ReactionTarget::Review, "t2", ReactionType::Upvote).is_ok());
}

#[tokio::test]
async fn test_d4_17_http_post_rate_limit_exceeded_returns_429() {
    let server = setup_test_app();
    let wallet = "0x417";
    let token = create_auth_token(wallet);
    REACTION_STORE.set_rate_limit(2, Duration::from_secs(60));

    server
        .post("/reactions/review/target-d4-17a")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    server
        .post("/reactions/review/target-d4-17b")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;

    let res3 = server
        .post("/reactions/review/target-d4-17c")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: "upvote".to_string(),
        })
        .await;
    assert_eq!(res3.status_code(), StatusCode::TOO_MANY_REQUESTS);

    // Reset rate limits back to default for other tests
    REACTION_STORE.reset_rate_limits();
    REACTION_STORE.set_rate_limit(50, Duration::from_secs(60));
}

#[test]
fn test_d4_18_cannot_switch_reaction_on_own_content() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Review, "rev-418", "0x418");
    let res = store.record_reaction("0x418", ReactionTarget::Review, "rev-418", ReactionType::Thanks);
    assert!(matches!(res, Err(backend::libs::error::ApiError::Forbidden(_))));
}

#[test]
fn test_d4_19_target_author_override_updates_policy() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Review, "rev-419", "0xoriginal");
    assert!(store.record_reaction("0xnew_author", ReactionTarget::Review, "rev-419", ReactionType::Upvote).is_ok());
    store.set_target_author(ReactionTarget::Review, "rev-419", "0xnew_author");
    let res = store.record_reaction("0xnew_author", ReactionTarget::Review, "rev-419", ReactionType::Insightful);
    assert!(matches!(res, Err(backend::libs::error::ApiError::Forbidden(_))));
}

#[test]
fn test_d4_20_valid_jwt_starknet_felt_normalization() {
    let wallet = "0x01"; // starknet felt
    let token = create_auth_token(wallet);
    let secret = backend::libs::jwt::secret_from_env();
    let claims = backend::libs::jwt::decode(&token, &secret).unwrap();
    let normalized = backend::libs::wallet::normalize_and_validate(&claims.sub).unwrap();
    assert!(normalized.starts_with("0x"));
}

// =========================================================================
// D5: STRESS, CONCURRENCY & HIGH LOAD (20 tests)
// =========================================================================

#[tokio::test]
async fn test_d5_01_stress_50_concurrent_requests_to_single_post() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(100, Duration::from_secs(60));
    let target_id = "target-stress-50";

    let mut handles = Vec::new();
    for i in 0..50 {
        let store_clone = Arc::clone(&store);
        let user_wallet = format!("0x501_{:03}", i);
        handles.push(tokio::spawn(async move {
            store_clone.record_reaction(&user_wallet, ReactionTarget::Review, target_id, ReactionType::Upvote)
        }));
    }

    for h in handles {
        let res = h.await.unwrap();
        assert!(res.is_ok());
    }

    let summary = store.get_summary(None, ReactionTarget::Review, target_id);
    assert_eq!(summary.upvotes, 50);
    assert_eq!(summary.total, 50);
}

#[tokio::test]
async fn test_d5_02_concurrency_50_users_simultaneous_upvotes() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(100, Duration::from_secs(60));
    let mut handles = Vec::new();

    for i in 0..50 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let user = format!("0x502_{}", i);
            store_clone.record_reaction(&user, ReactionTarget::Comment, "com-50", ReactionType::Insightful)
        }));
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }

    let summary = store.get_summary(None, ReactionTarget::Comment, "com-50");
    assert_eq!(summary.insightful, 50);
}

#[tokio::test]
async fn test_d5_03_concurrency_50_users_simultaneous_switches() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(200, Duration::from_secs(60));
    // Initial upvote for 50 users
    for i in 0..50 {
        store.record_reaction(&format!("0x503_{}", i), ReactionTarget::Review, "t503", ReactionType::Upvote).unwrap();
    }

    // Simultaneously switch all 50 users to Thanks
    let mut handles = Vec::new();
    for i in 0..50 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let user = format!("0x503_{}", i);
            store_clone.record_reaction(&user, ReactionTarget::Review, "t503", ReactionType::Thanks)
        }));
    }

    for h in handles {
        assert_eq!(h.await.unwrap().unwrap().status, "switched");
    }

    let summary = store.get_summary(None, ReactionTarget::Review, "t503");
    assert_eq!(summary.upvotes, 0);
    assert_eq!(summary.thanks, 50);
    assert_eq!(summary.total, 50);
}

#[tokio::test]
async fn test_d5_04_concurrency_race_condition_sum_of_types_equals_total() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(200, Duration::from_secs(60));
    let mut handles = Vec::new();

    for i in 0..60 {
        let store_clone = Arc::clone(&store);
        let reaction = match i % 3 {
            0 => ReactionType::Upvote,
            1 => ReactionType::Insightful,
            _ => ReactionType::Thanks,
        };
        handles.push(tokio::spawn(async move {
            let user = format!("0x504_{}", i);
            store_clone.record_reaction(&user, ReactionTarget::Review, "t504", reaction)
        }));
    }

    for h in handles {
        h.await.unwrap().unwrap();
    }

    let summary = store.get_summary(None, ReactionTarget::Review, "t504");
    assert_eq!(summary.upvotes, 20);
    assert_eq!(summary.insightful, 20);
    assert_eq!(summary.thanks, 20);
    assert_eq!(summary.total, 60);
}

#[tokio::test]
async fn test_d5_05_concurrency_rapid_repeated_reactions_same_user() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(100, Duration::from_secs(60));
    let mut handles = Vec::new();

    for _ in 0..30 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            store_clone.record_reaction("0x505_single", ReactionTarget::Review, "t505", ReactionType::Upvote)
        }));
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }

    let summary = store.get_summary(None, ReactionTarget::Review, "t505");
    assert_eq!(summary.upvotes, 1);
    assert_eq!(summary.total, 1);
}

#[tokio::test]
async fn test_d5_06_concurrency_simultaneous_add_and_delete() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(200, Duration::from_secs(60));

    // Pre-populate 25 users
    for i in 0..25 {
        store.record_reaction(&format!("0x506_{}", i), ReactionTarget::Review, "t506", ReactionType::Upvote).unwrap();
    }

    let mut handles = Vec::new();
    // 25 deletes
    for i in 0..25 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let user = format!("0x506_{}", i);
            store_clone.remove_reaction(&user, ReactionTarget::Review, "t506")
        }));
    }
    // 25 new adds
    for i in 25..50 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let user = format!("0x506_{}", i);
            store_clone.record_reaction(&user, ReactionTarget::Review, "t506", ReactionType::Thanks)
        }));
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }

    let summary = store.get_summary(None, ReactionTarget::Review, "t506");
    assert_eq!(summary.upvotes, 0);
    assert_eq!(summary.thanks, 25);
    assert_eq!(summary.total, 25);
}

#[tokio::test]
async fn test_d5_07_stress_concurrent_reactions_across_10_targets() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(200, Duration::from_secs(60));
    let mut handles = Vec::new();

    for target_idx in 0..10 {
        for user_idx in 0..10 {
            let store_clone = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                let target = format!("target_{}", target_idx);
                let user = format!("user_{}_{}", target_idx, user_idx);
                store_clone.record_reaction(&user, ReactionTarget::Review, &target, ReactionType::Upvote)
            }));
        }
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }

    for target_idx in 0..10 {
        let target = format!("target_{}", target_idx);
        let summary = store.get_summary(None, ReactionTarget::Review, &target);
        assert_eq!(summary.upvotes, 10);
    }
}

#[tokio::test]
async fn test_d5_08_concurrency_50_tasks_querying_summary_during_writes() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(200, Duration::from_secs(60));
    let mut handles = Vec::new();

    // 25 writers
    for i in 0..25 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let user = format!("0x508_w_{}", i);
            let _ = store_clone.record_reaction(&user, ReactionTarget::Comment, "t508", ReactionType::Insightful);
        }));
    }
    // 25 readers
    for _ in 0..25 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let s = store_clone.get_summary(None, ReactionTarget::Comment, "t508");
            assert!(s.total <= 25);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn test_d5_09_concurrency_counter_integrity_never_negative() {
    let store = Arc::new(ReactionStore::new());
    let mut handles = Vec::new();

    // Spawn 50 random deletes on an empty store
    for i in 0..50 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let user = format!("0x509_{}", i);
            store_clone.remove_reaction(&user, ReactionTarget::Review, "t509")
        }));
    }

    for h in handles {
        let res = h.await.unwrap().unwrap();
        assert_eq!(res.summary.total, 0);
    }
}

#[test]
fn test_d5_10_stress_100_distinct_users_on_single_review() {
    let store = ReactionStore::new();
    store.set_rate_limit(500, Duration::from_secs(60));
    for i in 0..100 {
        store.record_reaction(&format!("0x510_{}", i), ReactionTarget::Review, "t510", ReactionType::Upvote).unwrap();
    }
    let s = store.get_summary(None, ReactionTarget::Review, "t510");
    assert_eq!(s.upvotes, 100);
    assert_eq!(s.total, 100);
}

#[test]
fn test_d5_11_stress_high_frequency_toggling_maintains_invariants() {
    let store = ReactionStore::new();
    store.set_rate_limit(100, Duration::from_secs(60));
    let user = "0x511";
    for i in 0..20 {
        let reaction = if i % 2 == 0 { ReactionType::Upvote } else { ReactionType::Insightful };
        store.record_reaction(user, ReactionTarget::Review, "t511", reaction).unwrap();
    }
    let s = store.get_summary(None, ReactionTarget::Review, "t511");
    assert_eq!(s.total, 1);
}

#[tokio::test]
async fn test_d5_12_concurrency_multi_threaded_rate_limiter_safety() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(50, Duration::from_secs(60));
    let mut handles = Vec::new();

    for i in 0..20 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let user = format!("0x512_{}", i);
            store_clone.check_rate_limit(&user)
        }));
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }
}

#[test]
fn test_d5_13_stress_interleaved_reviews_and_comments() {
    let store = ReactionStore::new();
    store.set_rate_limit(200, Duration::from_secs(60));
    for i in 0..25 {
        store.record_reaction(&format!("0x513_{}", i), ReactionTarget::Review, "shared_item", ReactionType::Upvote).unwrap();
        store.record_reaction(&format!("0x513_{}", i), ReactionTarget::Comment, "shared_item", ReactionType::Thanks).unwrap();
    }
    let s_rev = store.get_summary(None, ReactionTarget::Review, "shared_item");
    let s_com = store.get_summary(None, ReactionTarget::Comment, "shared_item");
    assert_eq!(s_rev.upvotes, 25);
    assert_eq!(s_com.thanks, 25);
}

#[tokio::test]
async fn test_d5_14_concurrency_simultaneous_deletes_by_same_user() {
    let store = Arc::new(ReactionStore::new());
    store.record_reaction("0x514", ReactionTarget::Review, "t514", ReactionType::Upvote).unwrap();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            store_clone.remove_reaction("0x514", ReactionTarget::Review, "t514")
        }));
    }

    let mut removed_count = 0;
    let mut unchanged_count = 0;
    for h in handles {
        let res = h.await.unwrap().unwrap();
        if res.status == "removed" {
            removed_count += 1;
        } else if res.status == "unchanged" {
            unchanged_count += 1;
        }
    }

    assert_eq!(removed_count, 1);
    assert_eq!(unchanged_count, 9);
}

#[tokio::test]
async fn test_d5_15_stress_rapid_http_calls_through_test_server() {
    let server = setup_test_app();
    REACTION_STORE.reset_rate_limits();
    REACTION_STORE.set_rate_limit(500, Duration::from_secs(60));

    for i in 0..10 {
        let wallet = format!("0x515{:02x}", i);
        let token = create_auth_token(&wallet);
        let res = server
            .post("/reactions/review/target-d5-15")
            .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
            .json(&CreateReactionDto {
                reaction_type: "upvote".to_string(),
            })
            .await;
        assert_eq!(res.status_code(), StatusCode::OK);
    }

    let summary_res = server.get("/reactions/review/target-d5-15").await;
    let s: ReactionSummary = summary_res.json();
    assert_eq!(s.upvotes, 10);

    REACTION_STORE.reset_rate_limits();
    REACTION_STORE.set_rate_limit(50, Duration::from_secs(60));
}

#[tokio::test]
async fn test_d5_16_stress_large_payload_rejection() {
    let server = setup_test_app();
    let token = create_auth_token("0x516");
    let huge_str = "a".repeat(10_000);
    let res = server
        .post("/reactions/review/target-d5-16")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&CreateReactionDto {
            reaction_type: huge_str,
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d5_17_concurrency_rwlock_no_deadlocks_under_load() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(200, Duration::from_secs(60));
    let mut handles = Vec::new();

    for i in 0..40 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let u = format!("0x517_{}", i);
            let _ = store_clone.record_reaction(&u, ReactionTarget::Review, "deadlock_test", ReactionType::Upvote);
            let _ = store_clone.get_summary(Some(&u), ReactionTarget::Review, "deadlock_test");
            let _ = store_clone.remove_reaction(&u, ReactionTarget::Review, "deadlock_test");
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[test]
fn test_d5_18_concurrency_atomic_target_author_checks() {
    let store = ReactionStore::new();
    store.set_target_author(ReactionTarget::Review, "t518", "0xauthor518");
    assert_eq!(store.get_target_author(ReactionTarget::Review, "t518"), Some("0xauthor518".to_string()));
    assert_eq!(store.get_target_author(ReactionTarget::Comment, "t518"), None);
}

#[tokio::test]
async fn test_d5_19_stress_mixed_workload_add_switch_delete_read() {
    let store = Arc::new(ReactionStore::new());
    store.set_rate_limit(500, Duration::from_secs(60));
    let mut handles = Vec::new();

    for i in 0..30 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let user = format!("0x519_{}", i);
            // 1. Add Upvote
            let _ = store_clone.record_reaction(&user, ReactionTarget::Review, "mix", ReactionType::Upvote);
            // 2. Switch to Insightful
            let _ = store_clone.record_reaction(&user, ReactionTarget::Review, "mix", ReactionType::Insightful);
            // 3. Read
            let _ = store_clone.get_summary(Some(&user), ReactionTarget::Review, "mix");
            // 4. Delete
            if i % 2 == 0 {
                let _ = store_clone.remove_reaction(&user, ReactionTarget::Review, "mix");
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let summary = store.get_summary(None, ReactionTarget::Review, "mix");
    // Odd users (15) kept their Insightful reaction
    assert_eq!(summary.insightful, 15);
    assert_eq!(summary.upvotes, 0);
    assert_eq!(summary.total, 15);
}

#[test]
fn test_d5_20_stress_end_to_end_consistency_audit() {
    let store = ReactionStore::new();
    store.set_rate_limit(500, Duration::from_secs(60));

    // Seed 10 reviews and 10 comments with various reactions
    for t_idx in 0..10 {
        let r_id = format!("audit_rev_{}", t_idx);
        let c_id = format!("audit_com_{}", t_idx);

        store.record_reaction("0xaudit_1", ReactionTarget::Review, &r_id, ReactionType::Upvote).unwrap();
        store.record_reaction("0xaudit_2", ReactionTarget::Review, &r_id, ReactionType::Insightful).unwrap();
        store.record_reaction("0xaudit_3", ReactionTarget::Comment, &c_id, ReactionType::Thanks).unwrap();
    }

    for t_idx in 0..10 {
        let r_id = format!("audit_rev_{}", t_idx);
        let c_id = format!("audit_com_{}", t_idx);

        let s_rev = store.get_summary(None, ReactionTarget::Review, &r_id);
        let s_com = store.get_summary(None, ReactionTarget::Comment, &c_id);

        assert_eq!(s_rev.total, 2);
        assert_eq!(s_rev.upvotes, 1);
        assert_eq!(s_rev.insightful, 1);

        assert_eq!(s_com.total, 1);
        assert_eq!(s_com.thanks, 1);
    }
}
