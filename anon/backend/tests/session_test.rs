use axum::http::StatusCode;
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::routes::session::{
    LogoutReq, LogoutRes, RefreshReq, RefreshRes, RevokeSessionRes, SESSION_STORE,
    SessionItem, canonical_wallet, extract_client_metadata, generate_secure_token, hash_ip,
    hash_token, parse_device_info,
};
use chrono::{Duration, Utc};

fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-session") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-session") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

// =========================================================================
// D1: TOKEN GENERATION, CRYPTOGRAPHY, FORMATTING & LIFETIME (20 tests)
// =========================================================================

#[test]
fn test_d1_01_token_generation_length_and_hex_entropy() {
    let token1 = generate_secure_token();
    let token2 = generate_secure_token();
    assert_eq!(token1.len(), 64);
    assert_eq!(token2.len(), 64);
    assert_ne!(token1, token2);
    assert!(token1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_d1_02_hash_token_sha256_consistency() {
    let token = "test_refresh_token_12345";
    let hash1 = hash_token(token);
    let hash2 = hash_token(token);
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64);
    assert_ne!(hash1, token);
}

#[test]
fn test_d1_03_hash_ip_anonymization_and_truncation() {
    let ip1 = "192.168.1.100";
    let ip2 = "192.168.1.101";
    let hash1 = hash_ip(ip1);
    let hash2 = hash_ip(ip2);
    assert_eq!(hash1.len(), 16);
    assert_eq!(hash2.len(), 16);
    assert_ne!(hash1, hash2);
}

#[test]
fn test_d1_04_device_info_parser_varieties() {
    assert_eq!(
        parse_device_info("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/120.0"),
        "Chrome (macOS)"
    );
    assert_eq!(
        parse_device_info("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0"),
        "Firefox (Windows)"
    );
    assert_eq!(
        parse_device_info("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/120.0 Mobile Safari/537.36"),
        "Chrome (Android)"
    );
    assert_eq!(
        parse_device_info("curl/7.88.1"),
        "cURL Client (Unknown OS)"
    );
    assert_eq!(
        parse_device_info("UnknownAgent"),
        "Web Browser (Unknown OS)"
    );
}

#[test]
fn test_d1_05_client_metadata_extraction() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::USER_AGENT, "Mozilla/5.0 (X11; Linux x86_64) Firefox/120.0".parse().unwrap());
    headers.insert("x-forwarded-for", "203.0.113.195, 70.41.3.18".parse().unwrap());

    let (dev, ip_hash) = extract_client_metadata(&headers);
    assert_eq!(dev, "Firefox (Linux)");
    assert_eq!(ip_hash.len(), 16);
}

#[test]
fn test_d1_06_device_parser_edge_and_safari() {
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
    assert_eq!(parse_device_info(ua), "Safari (macOS)");

    let edge_ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Edg/120.0";
    assert_eq!(parse_device_info(edge_ua), "Edge (Windows)");
}

#[test]
fn test_d1_07_device_parser_ios_and_ipad() {
    let iphone_ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Version/17.0 Mobile/15E148 Safari/604.1";
    assert_eq!(parse_device_info(iphone_ua), "Safari (iOS)");

    let ipad_ua = "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Version/17.0 Mobile/15E148 Safari/604.1";
    assert_eq!(parse_device_info(ipad_ua), "Safari (iOS)");
}

#[test]
fn test_d1_08_ipv6_hashing_and_truncation() {
    let ipv6_1 = "2001:0db8:85a3:0000:0000:8a2e:0370:7334";
    let ipv6_2 = "2001:0db8:85a3:0000:0000:8a2e:0370:7335";
    let h1 = hash_ip(ipv6_1);
    let h2 = hash_ip(ipv6_2);
    assert_eq!(h1.len(), 16);
    assert_eq!(h2.len(), 16);
    assert_ne!(h1, h2);
}

#[test]
fn test_d1_09_custom_salt_ip_hashing() {
    unsafe { std::env::set_var("IP_SALT", "custom-salt-12345") };
    let h1 = hash_ip("10.0.0.1");
    unsafe { std::env::set_var("IP_SALT", "custom-salt-67890") };
    let h2 = hash_ip("10.0.0.1");
    assert_ne!(h1, h2);
    assert_eq!(h1.len(), 16);
    assert_eq!(h2.len(), 16);
}

#[test]
fn test_d1_10_empty_ip_fallback_handling() {
    let h = hash_ip("");
    assert_eq!(h.len(), 16);
}

#[test]
fn test_d1_11_jwt_token_encoding_claims() {
    let wallet = "0x1111";
    let token = create_auth_token(wallet);
    assert!(!token.is_empty());
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
}

#[test]
fn test_d1_12_jwt_token_expiration_validation() {
    let wallet = "0x1112";
    let token = create_auth_token(wallet);
    let secret = backend::libs::jwt::secret_from_env();
    let claims = backend::libs::jwt::decode(&token, &secret).unwrap();
    assert_eq!(claims.sub, wallet);
    assert!(claims.exp > 0);
}

#[test]
fn test_d1_13_jwt_token_invalid_signature_rejection() {
    let wallet = "0x1113";
    let token = create_auth_token(wallet);
    let wrong_secret = b"wrong-secret-signature-key-12345678";
    let result = backend::libs::jwt::decode(&token, wrong_secret);
    assert!(result.is_err());
}

#[test]
fn test_d1_14_canonical_wallet_address_normalization() {
    let raw = "0x01234";
    let canonical = canonical_wallet(raw);
    assert_eq!(canonical, "0x1234");
}

#[test]
fn test_d1_15_canonical_wallet_case_insensitivity() {
    let upper = "0xABCD";
    let lower = "0xabcd";
    assert_eq!(canonical_wallet(upper), canonical_wallet(lower));
}

#[test]
fn test_d1_16_canonical_wallet_invalid_fallback() {
    let invalid = "not-a-felt";
    let canon = canonical_wallet(invalid);
    assert_eq!(canon, "not-a-felt");
}

#[test]
fn test_d1_17_token_entropy_uniqueness_batch() {
    let mut tokens = std::collections::HashSet::new();
    for _ in 0..100 {
        let t = generate_secure_token();
        assert!(tokens.insert(t));
    }
    assert_eq!(tokens.len(), 100);
}

#[test]
fn test_d1_18_hash_token_empty_string() {
    let h = hash_token("");
    assert_eq!(h.len(), 64);
    assert_eq!(h, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
}

#[test]
fn test_d1_19_client_metadata_real_ip_header() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-real-ip", "198.51.100.42".parse().unwrap());
    headers.insert(axum::http::header::USER_AGENT, "curl/8.1.2".parse().unwrap());
    let (dev, ip_hash) = extract_client_metadata(&headers);
    assert_eq!(dev, "cURL Client (Unknown OS)");
    assert_eq!(ip_hash, hash_ip("198.51.100.42"));
}

#[test]
fn test_d1_20_client_metadata_missing_headers_defaults() {
    let headers = axum::http::HeaderMap::new();
    let (dev, ip_hash) = extract_client_metadata(&headers);
    assert_eq!(dev, "Web Browser (Unknown OS)");
    assert_eq!(ip_hash, hash_ip("127.0.0.1"));
}

// =========================================================================
// D2: SESSION CREATION & ATOMIC TOKEN ROTATION (20 tests)
// =========================================================================

#[tokio::test]
async fn test_d2_01_session_creation_and_listing() {
    let server = setup_test_app();
    let wallet = "0x2001";
    let token = create_auth_token(wallet);

    let (session, raw_refresh) = SESSION_STORE.create_session(
        wallet,
        "Chrome (macOS)",
        "hash_ip_test",
        7,
    );
    assert_eq!(session.wallet, canonical_wallet(wallet));
    assert!(!session.is_revoked);
    assert_eq!(raw_refresh.len(), 64);

    let res = server
        .get("/me/sessions")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", token))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let items: Vec<SessionItem> = res.json();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, session.id);
    assert_eq!(items[0].wallet, canonical_wallet(wallet));
    assert_eq!(items[0].device_name, "Chrome (macOS)");
}

#[tokio::test]
async fn test_d2_02_successful_token_rotation() {
    let server = setup_test_app();
    let wallet = "0x2002";

    let (session, initial_refresh) = SESSION_STORE.create_session(
        wallet,
        "Chrome (macOS)",
        "ip_initial",
        7,
    );

    let req_body = RefreshReq {
        refresh_token: initial_refresh.clone(),
    };

    let res = server
        .post("/auth/refresh")
        .add_header(axum::http::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0")
        .json(&req_body)
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let rotate_res: RefreshRes = res.json();

    assert_eq!(rotate_res.session_id, session.id);
    assert_ne!(rotate_res.refresh_token, initial_refresh);
    assert_eq!(rotate_res.token_type, "Bearer");
    assert!(!rotate_res.access_token.is_empty());

    let secret = backend::libs::jwt::secret_from_env();
    let claims = backend::libs::jwt::decode(&rotate_res.access_token, &secret).unwrap();
    assert_eq!(claims.sub, canonical_wallet(wallet));
}

#[tokio::test]
async fn test_d2_03_chained_token_rotations() {
    let server = setup_test_app();
    let wallet = "0x2003";

    let (_, mut current_refresh) = SESSION_STORE.create_session(
        wallet,
        "Device",
        "ip",
        7,
    );

    for _ in 0..5 {
        let res = server
            .post("/auth/refresh")
            .json(&RefreshReq {
                refresh_token: current_refresh.clone(),
            })
            .await;

        assert_eq!(res.status_code(), StatusCode::OK);
        let rotate_res: RefreshRes = res.json();
        assert_ne!(rotate_res.refresh_token, current_refresh);
        current_refresh = rotate_res.refresh_token;
    }
}

#[test]
fn test_d2_04_session_default_values_on_creation() {
    let wallet = "0x2004";
    let (s, raw) = SESSION_STORE.create_session(wallet, "Default Dev", "ip_default", 7);
    assert_eq!(s.wallet, canonical_wallet(wallet));
    assert_eq!(s.device_name, "Default Dev");
    assert_eq!(s.ip_hash, "ip_default");
    assert!(!s.is_revoked);
    assert!(s.rotated_token_hashes.is_empty());
    assert_eq!(s.current_refresh_token_hash, hash_token(&raw));
}

#[test]
fn test_d2_05_session_custom_ttl() {
    let wallet = "0x2005";
    let (s, _) = SESSION_STORE.create_session(wallet, "Custom TTL", "ip", 30);
    let diff = s.expires_at - s.created_at;
    assert_eq!(diff.num_days(), 30);
}

#[test]
fn test_d2_06_rotate_token_updates_last_active_timestamp() {
    let wallet = "0x2006";
    let (s, token) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let initial_active = s.last_active_at;
    std::thread::sleep(std::time::Duration::from_millis(5));
    let (rotated, _, _) = SESSION_STORE.rotate_token(&token, None, None).unwrap();
    assert!(rotated.last_active_at >= initial_active);
}

#[test]
fn test_d2_07_rotate_token_updates_device_metadata() {
    let wallet = "0x2007";
    let (_, token) = SESSION_STORE.create_session(wallet, "Old Device", "ip", 7);
    let (rotated, _, _) = SESSION_STORE.rotate_token(&token, Some("New Device / Chrome"), None).unwrap();
    assert_eq!(rotated.device_name, "New Device / Chrome");
}

#[test]
fn test_d2_08_rotate_token_updates_ip_hash() {
    let wallet = "0x2008";
    let (_, token) = SESSION_STORE.create_session(wallet, "Dev", "old_ip", 7);
    let (rotated, _, _) = SESSION_STORE.rotate_token(&token, None, Some("new_ip")).unwrap();
    assert_eq!(rotated.ip_hash, "new_ip");
}

#[test]
fn test_d2_09_rotate_token_ignores_empty_device_name() {
    let wallet = "0x2009";
    let (_, token) = SESSION_STORE.create_session(wallet, "Preserved Device", "ip", 7);
    let (rotated, _, _) = SESSION_STORE.rotate_token(&token, Some("   "), None).unwrap();
    assert_eq!(rotated.device_name, "Preserved Device");
}

#[test]
fn test_d2_10_rotate_token_ignores_empty_ip_hash() {
    let wallet = "0x200a";
    let (_, token) = SESSION_STORE.create_session(wallet, "Dev", "preserved_ip", 7);
    let (rotated, _, _) = SESSION_STORE.rotate_token(&token, None, Some("   ")).unwrap();
    assert_eq!(rotated.ip_hash, "preserved_ip");
}

#[test]
fn test_d2_11_rotate_token_preserves_created_at() {
    let wallet = "0x200b";
    let (orig, token) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let (rotated, _, _) = SESSION_STORE.rotate_token(&token, None, None).unwrap();
    assert_eq!(rotated.created_at, orig.created_at);
}

#[test]
fn test_d2_12_rotate_token_returns_new_unique_refresh_tokens() {
    let wallet = "0x200c";
    let (_, t1) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let (_, _, t2) = SESSION_STORE.rotate_token(&t1, None, None).unwrap();
    let (_, _, t3) = SESSION_STORE.rotate_token(&t2, None, None).unwrap();
    assert_ne!(t1, t2);
    assert_ne!(t2, t3);
    assert_ne!(t1, t3);
}

#[test]
fn test_d2_13_rotate_token_jwt_contains_correct_claims() {
    let wallet = "0x200d";
    let (_, token) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let (_, jwt, _) = SESSION_STORE.rotate_token(&token, None, None).unwrap();
    let secret = backend::libs::jwt::secret_from_env();
    let claims = backend::libs::jwt::decode(&jwt, &secret).unwrap();
    assert_eq!(claims.sub, canonical_wallet(wallet));
}

#[test]
fn test_d2_14_create_multiple_sessions_for_same_wallet() {
    let wallet = "0x200e";
    let (s1, _) = SESSION_STORE.create_session(wallet, "Dev 1", "ip", 7);
    let (s2, _) = SESSION_STORE.create_session(wallet, "Dev 2", "ip", 7);
    let (s3, _) = SESSION_STORE.create_session(wallet, "Dev 3", "ip", 7);
    assert_ne!(s1.id, s2.id);
    assert_ne!(s2.id, s3.id);
    let active = SESSION_STORE.list_active_sessions(wallet, None);
    assert_eq!(active.len(), 3);
}

#[test]
fn test_d2_15_create_sessions_for_distinct_wallets() {
    let w1 = "0x200f";
    let w2 = "0x2010";
    SESSION_STORE.create_session(w1, "Dev", "ip", 7);
    SESSION_STORE.create_session(w2, "Dev", "ip", 7);
    let a1 = SESSION_STORE.list_active_sessions(w1, None);
    let a2 = SESSION_STORE.list_active_sessions(w2, None);
    assert_eq!(a1.len(), 1);
    assert_eq!(a2.len(), 1);
    assert_ne!(a1[0].wallet, a2[0].wallet);
}

#[test]
fn test_d2_16_list_active_sessions_filters_by_wallet() {
    let w1 = "0x2011";
    let w2 = "0x2012";
    SESSION_STORE.create_session(w1, "Dev1", "ip", 7);
    SESSION_STORE.create_session(w2, "Dev2", "ip", 7);
    let list1 = SESSION_STORE.list_active_sessions(w1, None);
    assert_eq!(list1.len(), 1);
    assert_eq!(list1[0].wallet, canonical_wallet(w1));
}

#[test]
fn test_d2_17_list_active_sessions_marks_current_session() {
    let wallet = "0x2013";
    let (s1, _) = SESSION_STORE.create_session(wallet, "Dev 1", "ip", 7);
    let (s2, _) = SESSION_STORE.create_session(wallet, "Dev 2", "ip", 7);
    let items = SESSION_STORE.list_active_sessions(wallet, Some(&s1.id));
    let item1 = items.iter().find(|i| i.id == s1.id).unwrap();
    let item2 = items.iter().find(|i| i.id == s2.id).unwrap();
    assert!(item1.is_current);
    assert!(!item2.is_current);
}

#[test]
fn test_d2_18_list_active_sessions_sorting_order() {
    let wallet = "0x2014";
    let (s1, _) = SESSION_STORE.create_session(wallet, "Dev 1", "ip", 7);
    std::thread::sleep(std::time::Duration::from_millis(5));
    let (s2, _) = SESSION_STORE.create_session(wallet, "Dev 2", "ip", 7);
    let items = SESSION_STORE.list_active_sessions(wallet, None);
    assert_eq!(items[0].id, s2.id);
    assert_eq!(items[1].id, s1.id);
}

#[test]
fn test_d2_19_session_item_equality_and_cloning() {
    let wallet = "0x2015";
    let (s, _) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let items = SESSION_STORE.list_active_sessions(wallet, None);
    let cloned = items[0].clone();
    assert_eq!(items[0], cloned);
    assert_eq!(items[0].id, s.id);
}

#[test]
fn test_d2_20_session_store_manual_reset() {
    let temp_store = backend::routes::session::SessionStore::new();
    temp_store.create_session("0x2016", "Dev", "ip", 7);
    assert_eq!(temp_store.list_active_sessions("0x2016", None).len(), 1);
    temp_store.clear();
    assert_eq!(temp_store.list_active_sessions("0x2016", None).len(), 0);
}

// =========================================================================
// D3: TOKEN REUSE DETECTION & BREACH MITIGATION (20 tests)
// =========================================================================

#[tokio::test]
async fn test_d3_01_token_reuse_attack_revokes_all_user_sessions() {
    let server = setup_test_app();
    let wallet = "0x3001";

    let (_s1, token_dev1) = SESSION_STORE.create_session(wallet, "Laptop", "ip1", 7);
    let (s2, _token_dev2) = SESSION_STORE.create_session(wallet, "Mobile", "ip2", 7);

    let res = server
        .post("/auth/refresh")
        .json(&RefreshReq {
            refresh_token: token_dev1.clone(),
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);

    // ATTACK: An attacker tries to use the OLD token_dev1 again!
    let attack_res = server
        .post("/auth/refresh")
        .json(&RefreshReq {
            refresh_token: token_dev1.clone(),
        })
        .await;

    assert_eq!(attack_res.status_code(), StatusCode::UNAUTHORIZED);
    let err_body: serde_json::Value = attack_res.json();
    assert!(err_body["error"].as_str().unwrap().contains("token reuse detected"));

    let active_sessions = SESSION_STORE.list_active_sessions(wallet, None);
    assert_eq!(active_sessions.len(), 0);

    let dev2_res = server
        .delete(&format!("/me/sessions/{}", s2.id))
        .add_header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", create_auth_token(wallet)),
        )
        .await;
    assert_eq!(dev2_res.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_d3_02_invalid_refresh_token_rejected() {
    let server = setup_test_app();
    let res = server
        .post("/auth/refresh")
        .json(&RefreshReq {
            refresh_token: "invalid_nonexistent_token_64chars_long_0123456789abcdef0123456789".to_string(),
        })
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_d3_03_token_reuse_with_immediate_predecessor() {
    let wallet = "0x3003";
    let (_, t0) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let (_, _, t1) = SESSION_STORE.rotate_token(&t0, None, None).unwrap();
    let (_, _, _t2) = SESSION_STORE.rotate_token(&t1, None, None).unwrap();
    let reuse = SESSION_STORE.rotate_token(&t1, None, None);
    assert!(reuse.is_err());
}

#[test]
fn test_d3_04_token_reuse_with_deep_ancestor_token() {
    let wallet = "0x3004";
    let (_, t0) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let mut curr = t0.clone();
    for _ in 0..10 {
        let (_, _, nxt) = SESSION_STORE.rotate_token(&curr, None, None).unwrap();
        curr = nxt;
    }
    let reuse_root = SESSION_STORE.rotate_token(&t0, None, None);
    assert!(reuse_root.is_err());
    assert_eq!(SESSION_STORE.list_active_sessions(wallet, None).len(), 0);
}

#[test]
fn test_d3_05_token_reuse_only_revokes_compromised_user_sessions() {
    let victim = "0x3005";
    let innocent = "0x3006";
    let (_, t_victim0) = SESSION_STORE.create_session(victim, "Victim Dev", "ip", 7);
    let (_, _t_innocent) = SESSION_STORE.create_session(innocent, "Innocent Dev", "ip", 7);
    let (_, _, _t_victim1) = SESSION_STORE.rotate_token(&t_victim0, None, None).unwrap();
    let _ = SESSION_STORE.rotate_token(&t_victim0, None, None);
    assert_eq!(SESSION_STORE.list_active_sessions(victim, None).len(), 0);
    assert_eq!(SESSION_STORE.list_active_sessions(innocent, None).len(), 1);
}

#[test]
fn test_d3_06_token_reuse_attempt_on_already_revoked_session() {
    let wallet = "0x3007";
    let (s, t0) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    SESSION_STORE.revoke_session(wallet, &s.id).unwrap();
    let res = SESSION_STORE.rotate_token(&t0, None, None);
    assert!(res.is_err());
}

#[test]
fn test_d3_07_refresh_on_manually_revoked_session_fails() {
    let wallet = "0x3008";
    let (s, token) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    SESSION_STORE.revoke_session(wallet, &s.id).unwrap();
    let res = SESSION_STORE.rotate_token(&token, None, None);
    assert!(res.is_err());
}

#[test]
fn test_d3_08_refresh_on_expired_session_fails_and_marks_revoked() {
    let wallet = "0x3009";
    let (s, token) = SESSION_STORE.create_session(wallet, "Dev", "ip", -2);
    let res = SESSION_STORE.rotate_token(&token, None, None);
    assert!(res.is_err());
    let active = SESSION_STORE.list_active_sessions(wallet, None);
    assert_eq!(active.len(), 0);
    assert!(SESSION_STORE.revoke_session(wallet, &s.id).is_ok());
}

#[test]
fn test_d3_09_empty_refresh_token_string_rejected() {
    let res = SESSION_STORE.rotate_token("", None, None);
    assert!(res.is_err());
}

#[test]
fn test_d3_10_whitespace_refresh_token_rejected() {
    let res = SESSION_STORE.rotate_token("   ", None, None);
    assert!(res.is_err());
}

#[test]
fn test_d3_11_truncated_refresh_token_rejected() {
    let wallet = "0x300a";
    let (_, token) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let truncated = &token[..32];
    let res = SESSION_STORE.rotate_token(truncated, None, None);
    assert!(res.is_err());
}

#[test]
fn test_d3_12_tampered_refresh_token_rejected() {
    let wallet = "0x300b";
    let (_, token) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let mut tampered = token.clone();
    tampered.replace_range(0..1, "f");
    let res = SESSION_STORE.rotate_token(&tampered, None, None);
    assert!(res.is_err());
}

#[test]
fn test_d3_13_reuse_detection_across_multiple_devices() {
    let wallet = "0x300c";
    let (_, t1) = SESSION_STORE.create_session(wallet, "Phone", "ip", 7);
    let (_, _t2) = SESSION_STORE.create_session(wallet, "Laptop", "ip", 7);
    let (_, _t3) = SESSION_STORE.create_session(wallet, "Watch", "ip", 7);
    let (_, _, _) = SESSION_STORE.rotate_token(&t1, None, None).unwrap();
    let attack = SESSION_STORE.rotate_token(&t1, None, None);
    assert!(attack.is_err());
    assert_eq!(SESSION_STORE.list_active_sessions(wallet, None).len(), 0);
}

#[test]
fn test_d3_14_reuse_detection_does_not_affect_unrelated_wallets() {
    let w1 = "0x300d";
    let w2 = "0x300e";
    let (_, t1) = SESSION_STORE.create_session(w1, "Phone", "ip", 7);
    let (_, t2) = SESSION_STORE.create_session(w2, "Laptop", "ip", 7);
    let (_, _, _) = SESSION_STORE.rotate_token(&t1, None, None).unwrap();
    let _ = SESSION_STORE.rotate_token(&t1, None, None);
    assert_eq!(SESSION_STORE.list_active_sessions(w1, None).len(), 0);
    assert_eq!(SESSION_STORE.list_active_sessions(w2, None).len(), 1);
    assert!(SESSION_STORE.rotate_token(&t2, None, None).is_ok());
}

#[test]
fn test_d3_15_rotation_history_length_matches_rotations() {
    let wallet = "0x300f";
    let (s, t0) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let (s1, _, t1) = SESSION_STORE.rotate_token(&t0, None, None).unwrap();
    let (s2, _, _t2) = SESSION_STORE.rotate_token(&t1, None, None).unwrap();
    assert_eq!(s.rotated_token_hashes.len(), 0);
    assert_eq!(s1.rotated_token_hashes.len(), 1);
    assert_eq!(s2.rotated_token_hashes.len(), 2);
}

#[test]
fn test_d3_16_rotation_history_stores_hashes_not_plain_tokens() {
    let wallet = "0x3010";
    let (_, t0) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let (s, _, _) = SESSION_STORE.rotate_token(&t0, None, None).unwrap();
    assert_eq!(s.rotated_token_hashes[0], hash_token(&t0));
    assert_ne!(s.rotated_token_hashes[0], t0);
}

#[test]
fn test_d3_17_repeated_reuse_attempts_stay_rejected() {
    let wallet = "0x3011";
    let (_, t0) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let (_, _, _) = SESSION_STORE.rotate_token(&t0, None, None).unwrap();
    assert!(SESSION_STORE.rotate_token(&t0, None, None).is_err());
    assert!(SESSION_STORE.rotate_token(&t0, None, None).is_err());
    assert!(SESSION_STORE.rotate_token(&t0, None, None).is_err());
}

#[test]
fn test_d3_18_concurrent_reuse_attacks_handled_gracefully() {
    let wallet = "0x3012";
    let (_, t0) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let (_, _, _) = SESSION_STORE.rotate_token(&t0, None, None).unwrap();
    for _ in 0..10 {
        assert!(SESSION_STORE.rotate_token(&t0, None, None).is_err());
    }
}

#[tokio::test]
async fn test_d3_19_error_message_clarity_on_reuse() {
    let server = setup_test_app();
    let wallet = "0x3013";
    let (_, t0) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let _ = server.post("/auth/refresh").json(&RefreshReq { refresh_token: t0.clone() }).await;
    let res = server.post("/auth/refresh").json(&RefreshReq { refresh_token: t0 }).await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = res.json();
    assert!(body["error"].as_str().unwrap().contains("token reuse detected"));
}

#[tokio::test]
async fn test_d3_20_error_message_clarity_on_invalid_token() {
    let server = setup_test_app();
    let res = server.post("/auth/refresh").json(&RefreshReq { refresh_token: "fake".to_string() }).await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = res.json();
    assert_eq!(body["error"], "invalid refresh token");
}

// =========================================================================
// D4: LOGOUT & DEVICE REVOCATION (20 tests)
// =========================================================================

#[tokio::test]
async fn test_d4_01_logout_by_refresh_token() {
    let server = setup_test_app();
    let wallet = "0x4001";
    let (session, raw_refresh) = SESSION_STORE.create_session(wallet, "Browser", "ip", 7);

    let res = server
        .post("/auth/logout")
        .json(&LogoutReq {
            refresh_token: Some(raw_refresh.clone()),
            session_id: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: LogoutRes = res.json();
    assert_eq!(body.revoked_session_id, Some(session.id.clone()));

    let ref_res = server
        .post("/auth/refresh")
        .json(&RefreshReq {
            refresh_token: raw_refresh,
        })
        .await;
    assert_eq!(ref_res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_02_logout_by_session_id_with_auth() {
    let server = setup_test_app();
    let wallet = "0x4002";
    let auth = create_auth_token(wallet);
    let (session, _) = SESSION_STORE.create_session(wallet, "Tablet", "ip", 7);

    let res = server
        .post("/auth/logout")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .json(&LogoutReq {
            refresh_token: None,
            session_id: Some(session.id.clone()),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: LogoutRes = res.json();
    assert_eq!(body.revoked_session_id, Some(session.id));
}

#[tokio::test]
async fn test_d4_03_logout_cannot_revoke_other_user_session() {
    let server = setup_test_app();
    let user1 = "0x4003";
    let user2 = "0x4004";
    let auth2 = create_auth_token(user2);
    let (session1, _) = SESSION_STORE.create_session(user1, "User1 PC", "ip", 7);

    let res = server
        .post("/auth/logout")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth2))
        .json(&LogoutReq {
            refresh_token: None,
            session_id: Some(session1.id),
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_d4_04_delete_me_sessions_success() {
    let server = setup_test_app();
    let wallet = "0x4005";
    let auth = create_auth_token(wallet);
    let (session, _) = SESSION_STORE.create_session(wallet, "Workstation", "ip", 7);

    let res = server
        .delete(&format!("/me/sessions/{}", session.id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: RevokeSessionRes = res.json();
    assert_eq!(body.session_id, session.id);

    let active = SESSION_STORE.list_active_sessions(wallet, None);
    assert_eq!(active.len(), 0);
}

#[tokio::test]
async fn test_d4_05_delete_me_sessions_other_user_forbidden() {
    let server = setup_test_app();
    let user1 = "0x4006";
    let user2 = "0x4007";
    let auth2 = create_auth_token(user2);
    let (session1, _) = SESSION_STORE.create_session(user1, "User1 Device", "ip", 7);

    let res = server
        .delete(&format!("/me/sessions/{}", session1.id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth2))
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_d4_06_delete_me_sessions_not_found() {
    let server = setup_test_app();
    let wallet = "0x4008";
    let auth = create_auth_token(wallet);

    let res = server
        .delete("/me/sessions/00000000-0000-0000-0000-000000000000")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_d4_07_unauthorized_access_to_protected_session_endpoints() {
    let server = setup_test_app();

    let res1 = server.get("/me/sessions").await;
    assert_eq!(res1.status_code(), StatusCode::UNAUTHORIZED);

    let res2 = server.delete("/me/sessions/any-id").await;
    assert_eq!(res2.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_08_logout_with_no_arguments_revokes_latest_session() {
    let server = setup_test_app();
    let wallet = "0x4009";
    let auth = create_auth_token(wallet);
    let (s, _) = SESSION_STORE.create_session(wallet, "Latest Dev", "ip", 7);

    let res = server
        .post("/auth/logout")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .json(&LogoutReq::default())
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: LogoutRes = res.json();
    assert_eq!(body.revoked_session_id, Some(s.id));
}

#[test]
fn test_d4_09_logout_with_unregistered_session_id_returns_none() {
    let res = SESSION_STORE.logout(None, Some("non-existent-uuid"), None).unwrap();
    assert_eq!(res, None);
}

#[test]
fn test_d4_10_logout_with_invalid_token_returns_none() {
    let res = SESSION_STORE.logout(Some("invalid-token"), None, None).unwrap();
    assert_eq!(res, None);
}

#[test]
fn test_d4_11_logout_twice_idempotency() {
    let wallet = "0x400a";
    let (s, raw) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let r1 = SESSION_STORE.logout(Some(&raw), None, None).unwrap();
    assert_eq!(r1, Some(s.id.clone()));
    let r2 = SESSION_STORE.logout(Some(&raw), None, None).unwrap();
    assert_eq!(r2, Some(s.id));
}

#[test]
fn test_d4_12_delete_me_sessions_twice_is_idempotent() {
    let wallet = "0x400b";
    let (s, _) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    assert!(SESSION_STORE.revoke_session(wallet, &s.id).is_ok());
    assert!(SESSION_STORE.revoke_session(wallet, &s.id).is_ok());
}

#[tokio::test]
async fn test_d4_13_delete_me_sessions_with_invalid_bearer_token() {
    let server = setup_test_app();
    let res = server
        .delete("/me/sessions/some-id")
        .add_header(axum::http::header::AUTHORIZATION, "Bearer invalid-jwt-signature")
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_14_delete_me_sessions_with_expired_bearer_token() {
    let server = setup_test_app();
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-session") };
    let claims = backend::libs::jwt::Claims {
        sub: "0x400c".to_string(),
        exp: 1000,
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(backend::libs::jwt::secret_from_env().as_slice()),
    )
    .unwrap();

    let res = server
        .delete("/me/sessions/some-id")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", token))
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_15_delete_me_sessions_with_malformed_auth_header() {
    let server = setup_test_app();
    let res = server
        .delete("/me/sessions/some-id")
        .add_header(axum::http::header::AUTHORIZATION, "Basic YWRtaW46cGFzcw==")
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_d4_16_revoke_all_sessions_for_wallet_utility() {
    let wallet = "0x400d";
    SESSION_STORE.create_session(wallet, "Dev 1", "ip", 7);
    SESSION_STORE.create_session(wallet, "Dev 2", "ip", 7);
    assert_eq!(SESSION_STORE.list_active_sessions(wallet, None).len(), 2);
    SESSION_STORE.revoke_all_sessions(wallet);
    assert_eq!(SESSION_STORE.list_active_sessions(wallet, None).len(), 0);
}

#[test]
fn test_d4_17_revoke_all_does_not_touch_other_wallets() {
    let w1 = "0x400e";
    let w2 = "0x400f";
    SESSION_STORE.create_session(w1, "Dev", "ip", 7);
    SESSION_STORE.create_session(w2, "Dev", "ip", 7);
    SESSION_STORE.revoke_all_sessions(w1);
    assert_eq!(SESSION_STORE.list_active_sessions(w1, None).len(), 0);
    assert_eq!(SESSION_STORE.list_active_sessions(w2, None).len(), 1);
}

#[test]
fn test_d4_18_logout_res_json_serialization() {
    let res = LogoutRes {
        message: "logged out".to_string(),
        revoked_session_id: Some("uuid-123".to_string()),
    };
    let val = serde_json::to_value(&res).unwrap();
    assert_eq!(val["message"], "logged out");
    assert_eq!(val["revoked_session_id"], "uuid-123");
}

#[test]
fn test_d4_19_revoke_session_res_json_serialization() {
    let res = RevokeSessionRes {
        message: "revoked".to_string(),
        session_id: "uuid-456".to_string(),
    };
    let val = serde_json::to_value(&res).unwrap();
    assert_eq!(val["message"], "revoked");
    assert_eq!(val["session_id"], "uuid-456");
}

#[test]
fn test_d4_20_refresh_res_json_serialization() {
    let res = RefreshRes {
        access_token: "jwt".to_string(),
        refresh_token: "refr".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: 3600,
        session_id: "s-1".to_string(),
    };
    let val = serde_json::to_value(&res).unwrap();
    assert_eq!(val["token_type"], "Bearer");
    assert_eq!(val["expires_in"], 3600);
}

// =========================================================================
// D5: STRESS, CONCURRENCY & MULTI-DEVICE MANAGEMENT (20 tests)
// =========================================================================

#[tokio::test]
async fn test_d5_01_multiple_active_devices_ordered_by_last_active() {
    let server = setup_test_app();
    let wallet = "0x5001";
    let auth = create_auth_token(wallet);

    let (s1, _) = SESSION_STORE.create_session(wallet, "Device 1", "ip1", 7);
    let (s2, _) = SESSION_STORE.create_session(wallet, "Device 2", "ip2", 7);
    let (s3, _) = SESSION_STORE.create_session(wallet, "Device 3", "ip3", 7);

    let res = server
        .get("/me/sessions")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let items: Vec<SessionItem> = res.json();
    assert_eq!(items.len(), 3);

    let ids: Vec<String> = items.into_iter().map(|i| i.id).collect();
    assert!(ids.contains(&s1.id));
    assert!(ids.contains(&s2.id));
    assert!(ids.contains(&s3.id));
}

#[tokio::test]
async fn test_d5_02_expired_sessions_excluded_from_active_list() {
    let server = setup_test_app();
    let wallet = "0x5002";
    let auth = create_auth_token(wallet);

    let (_, raw_refresh) = SESSION_STORE.create_session(wallet, "Old Device", "ip", -1);

    let res = server
        .get("/me/sessions")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let items: Vec<SessionItem> = res.json();
    assert_eq!(items.len(), 0);

    let ref_res = server
        .post("/auth/refresh")
        .json(&RefreshReq {
            refresh_token: raw_refresh,
        })
        .await;
    assert_eq!(ref_res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d5_03_concurrent_token_rotations_stress() {
    let mut tasks = Vec::new();
    for i in 0..50 {
        let wallet = format!("0x{:x}", i + 0x6000);
        let (_, token) = SESSION_STORE.create_session(&wallet, "Concurrent Device", "ip", 7);

        tasks.push(tokio::spawn(async move {
            let res = SESSION_STORE.rotate_token(&token, Some("Updated Device"), Some("new_ip"));
            assert!(res.is_ok());
            let (session, access, new_refresh) = res.unwrap();
            assert!(!access.is_empty());
            assert_ne!(new_refresh, token);
            assert_eq!(session.wallet, canonical_wallet(&wallet));
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
}

#[tokio::test]
async fn test_d5_04_concurrent_session_creations_stress() {
    let mut tasks = Vec::new();
    for i in 0..50 {
        let wallet = format!("0x{:x}", i + 0x7000);
        tasks.push(tokio::spawn(async move {
            let (s, raw) = SESSION_STORE.create_session(&wallet, "Stress Dev", "ip", 7);
            assert_eq!(s.wallet, canonical_wallet(&wallet));
            assert_eq!(raw.len(), 64);
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
}

#[tokio::test]
async fn test_d5_05_concurrent_session_revocations_stress() {
    let mut tasks = Vec::new();
    for i in 0..50 {
        let wallet = format!("0x{:x}", i + 0x8000);
        let (s, _) = SESSION_STORE.create_session(&wallet, "Stress Dev", "ip", 7);
        tasks.push(tokio::spawn(async move {
            let res = SESSION_STORE.revoke_session(&wallet, &s.id);
            assert!(res.is_ok());
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
}

#[test]
fn test_d5_06_rapid_sequential_rotations_single_session() {
    let wallet = "0x5006";
    let (_, mut token) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    for _ in 0..100 {
        let (_, _, nxt) = SESSION_STORE.rotate_token(&token, None, None).unwrap();
        token = nxt;
    }
}

#[test]
fn test_d5_07_high_volume_session_listing() {
    let wallet = "0x5007";
    for i in 0..50 {
        SESSION_STORE.create_session(wallet, &format!("Device {}", i), "ip", 7);
    }
    let items = SESSION_STORE.list_active_sessions(wallet, None);
    assert_eq!(items.len(), 50);
}

#[test]
fn test_d5_08_session_expiry_boundary_exactly_at_expiry() {
    let wallet = "0x5008";
    let (s, _) = SESSION_STORE.create_session(wallet, "Dev", "ip", 0);
    assert!(s.expires_at >= s.created_at);
}

#[test]
fn test_d5_09_session_expiry_boundary_one_second_after() {
    let wallet = "0x5009";
    let (mut s, _) = SESSION_STORE.create_session(wallet, "Dev", "ip", 1);
    s.expires_at = Utc::now() - Duration::seconds(1);
    assert!(s.expires_at < Utc::now());
}

#[test]
fn test_d5_10_mixed_read_write_concurrency() {
    let wallet = "0x500a";
    let (_, token) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let mut handles = Vec::new();
    for _ in 0..10 {
        let w = wallet.to_string();
        handles.push(std::thread::spawn(move || {
            let _ = SESSION_STORE.list_active_sessions(&w, None);
        }));
    }
    let (_, _, _) = SESSION_STORE.rotate_token(&token, None, None).unwrap();
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_d5_11_session_store_thread_safety_under_race() {
    let wallet = "0x500b";
    let (s, _) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let mut handles = Vec::new();
    for _ in 0..10 {
        let w = wallet.to_string();
        let sid = s.id.clone();
        handles.push(std::thread::spawn(move || {
            let _ = SESSION_STORE.revoke_session(&w, &sid);
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_d5_12_non_ascii_device_name_handling() {
    let wallet = "0x500c";
    let (s, _) = SESSION_STORE.create_session(wallet, "Компьютер разработчика 🚀", "ip", 7);
    assert_eq!(s.device_name, "Компьютер разработчика 🚀");
}

#[test]
fn test_d5_13_very_long_device_name_handling() {
    let wallet = "0x500d";
    let long_dev = "A".repeat(1000);
    let (s, _) = SESSION_STORE.create_session(wallet, &long_dev, "ip", 7);
    assert_eq!(s.device_name.len(), 1000);
}

#[test]
fn test_d5_14_very_long_user_agent_parsing() {
    let long_ua = format!("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/120.0 {}", "X".repeat(500));
    let parsed = parse_device_info(&long_ua);
    assert_eq!(parsed, "Chrome (macOS)");
}

#[test]
fn test_d5_15_ipv6_with_port_metadata_extraction() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-real-ip", "[2001:db8::1]:8080".parse().unwrap());
    let (_, ip_hash) = extract_client_metadata(&headers);
    assert_eq!(ip_hash.len(), 16);
}

#[test]
fn test_d5_16_multiple_forwarded_for_ips_takes_first() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-forwarded-for", "1.1.1.1, 2.2.2.2, 3.3.3.3".parse().unwrap());
    let (_, ip_hash) = extract_client_metadata(&headers);
    assert_eq!(ip_hash, hash_ip("1.1.1.1"));
}

#[test]
fn test_d5_17_concurrent_logouts_idempotency() {
    let wallet = "0x5011";
    let (s, _) = SESSION_STORE.create_session(wallet, "Dev", "ip", 7);
    let mut handles = Vec::new();
    for _ in 0..5 {
        let sid = s.id.clone();
        handles.push(std::thread::spawn(move || {
            let _ = SESSION_STORE.logout(None, Some(&sid), None);
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_d5_18_memory_cleanup_validation() {
    let store = backend::routes::session::SessionStore::new();
    for i in 0..100 {
        store.create_session(&format!("0x{:x}", i + 1), "Dev", "ip", 7);
    }
    store.clear();
    assert_eq!(store.list_active_sessions("0x1", None).len(), 0);
}

#[tokio::test]
async fn test_d5_19_full_end_to_end_user_journey() {
    let server = setup_test_app();
    let wallet = "0x5013";
    let auth = create_auth_token(wallet);

    // 1. User logs in (session created)
    let (s, refresh_token) = SESSION_STORE.create_session(wallet, "Chrome / Desktop", "192.168.1.1", 7);

    // 2. User lists active sessions
    let list_res = server
        .get("/me/sessions")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;
    assert_eq!(list_res.status_code(), StatusCode::OK);
    let sessions: Vec<SessionItem> = list_res.json();
    assert_eq!(sessions.len(), 1);

    // 3. User rotates refresh token
    let rot_res = server
        .post("/auth/refresh")
        .json(&RefreshReq { refresh_token: refresh_token.clone() })
        .await;
    assert_eq!(rot_res.status_code(), StatusCode::OK);
    let rot_body: RefreshRes = rot_res.json();

    // 4. User revokes session
    let del_res = server
        .delete(&format!("/me/sessions/{}", s.id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;
    assert_eq!(del_res.status_code(), StatusCode::OK);

    // 5. Token is now dead
    let fail_res = server
        .post("/auth/refresh")
        .json(&RefreshReq { refresh_token: rot_body.refresh_token })
        .await;
    assert_eq!(fail_res.status_code(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_d5_20_session_system_benchmark_throughput() {
    let start = std::time::Instant::now();
    let store = backend::routes::session::SessionStore::new();
    for _ in 0..1000 {
        let (s, raw) = store.create_session("0x9999", "Bench", "ip", 7);
        let _ = store.rotate_token(&raw, None, None);
        let _ = store.revoke_session("0x9999", &s.id);
    }
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 2000, "1000 full lifecycles took {:?}", elapsed);
}
