use tokio::sync::Mutex;

use axum::http::StatusCode;
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::models::company::{
    CompanyListResponse, CompanyResponse, CreateCompanyRequest, UpdateCompanyRequest,
};
use backend::services::company_service::{
    COMPANY_SERVICE, CompanyService, MAX_DESCRIPTION_LEN, MAX_NAME_LEN,
};

static TEST_LOCK: Mutex<()> = Mutex::const_new(());

fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-company-crud") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-company-crud") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

const ADMIN_WALLET: &str = "0x0123";
const NON_ADMIN_WALLET: &str = "0x0999999999";

// =========================================================================
// 1. SLUG GENERATION & NORMALIZATION TESTS
// =========================================================================

#[test]
fn test_01_slugify_basic() {
    let _guard = TEST_LOCK.blocking_lock();
    assert_eq!(CompanyService::slugify("StarkWare"), "starkware");
    assert_eq!(CompanyService::slugify("Stark Ware"), "stark-ware");
    assert_eq!(CompanyService::slugify("Stark-Ware"), "stark-ware");
}

#[test]
fn test_02_slugify_special_characters_and_whitespace() {
    let _guard = TEST_LOCK.blocking_lock();
    assert_eq!(
        CompanyService::slugify("StarkWare & Co. (Labs)!"),
        "starkware-co-labs"
    );
    assert_eq!(CompanyService::slugify("  Acme   Corp  "), "acme-corp");
    assert_eq!(CompanyService::slugify("Alpha---Beta___Gamma"), "alpha-beta-gamma");
}

#[test]
fn test_03_slugify_empty_or_symbols_fallback() {
    let _guard = TEST_LOCK.blocking_lock();
    assert_eq!(CompanyService::slugify(""), "company");
    assert_eq!(CompanyService::slugify("!@#$%^&*()"), "company");
    assert_eq!(CompanyService::slugify("---"), "company");
}

#[test]
fn test_04_slug_collision_resolution() {
    let _guard = TEST_LOCK.blocking_lock();
    COMPANY_SERVICE.reset();

    let c1 = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Argent Labs".to_string(),
            domain: None,
            description: None,
        })
        .unwrap();
    assert_eq!(c1.slug, "argent-labs");

    let c2 = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Argent Labs".to_string(),
            domain: None,
            description: None,
        })
        .unwrap();
    assert_eq!(c2.slug, "argent-labs-2");

    let c3 = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Argent Labs".to_string(),
            domain: None,
            description: None,
        })
        .unwrap();
    assert_eq!(c3.slug, "argent-labs-3");
}

// =========================================================================
// 2. NAME & DESCRIPTION VALIDATION TESTS
// =========================================================================

#[test]
fn test_05_name_validation_bounds() {
    let _guard = TEST_LOCK.blocking_lock();
    assert!(CompanyService::validate_name("").is_err());
    assert!(CompanyService::validate_name("a").is_err());
    assert!(CompanyService::validate_name("  a  ").is_err());

    // Exact min (2 chars)
    let min_name = CompanyService::validate_name("ab").unwrap();
    assert_eq!(min_name, "ab");

    // Exact max (80 chars)
    let max_str = "a".repeat(MAX_NAME_LEN);
    let max_name = CompanyService::validate_name(&max_str).unwrap();
    assert_eq!(max_name.len(), MAX_NAME_LEN);

    // Too long (81 chars)
    let too_long = "a".repeat(MAX_NAME_LEN + 1);
    assert!(CompanyService::validate_name(&too_long).is_err());
}

#[test]
fn test_06_description_validation() {
    let _guard = TEST_LOCK.blocking_lock();
    assert_eq!(CompanyService::validate_description(None).unwrap(), None);
    assert_eq!(
        CompanyService::validate_description(Some("   ")).unwrap(),
        None
    );

    let valid_desc = "Building decentralized Starknet infra.";
    assert_eq!(
        CompanyService::validate_description(Some(valid_desc)).unwrap(),
        Some(valid_desc.to_string())
    );

    let max_desc = "x".repeat(MAX_DESCRIPTION_LEN);
    assert!(CompanyService::validate_description(Some(&max_desc)).is_ok());

    let too_long_desc = "x".repeat(MAX_DESCRIPTION_LEN + 1);
    assert!(CompanyService::validate_description(Some(&too_long_desc)).is_err());
}

// =========================================================================
// 3. RFC DOMAIN VALIDATION TESTS
// =========================================================================

#[test]
fn test_07_domain_valid_formats() {
    let _guard = TEST_LOCK.blocking_lock();
    assert_eq!(
        CompanyService::validate_domain(Some("starkware.co")).unwrap(),
        Some("starkware.co".to_string())
    );
    assert_eq!(
        CompanyService::validate_domain(Some("SUB.DOMAIN.EXAMPLE.ORG")).unwrap(),
        Some("sub.domain.example.org".to_string())
    );
    assert_eq!(
        CompanyService::validate_domain(Some("my-company.tech")).unwrap(),
        Some("my-company.tech".to_string())
    );
}

#[test]
fn test_08_domain_invalid_formats() {
    let _guard = TEST_LOCK.blocking_lock();
    // Protocol not allowed
    assert!(CompanyService::validate_domain(Some("https://starkware.co")).is_err());
    assert!(CompanyService::validate_domain(Some("http://starkware.co")).is_err());
    // Port not allowed
    assert!(CompanyService::validate_domain(Some("starkware.co:8080")).is_err());
    // Path not allowed
    assert!(CompanyService::validate_domain(Some("starkware.co/path")).is_err());
    // No dot
    assert!(CompanyService::validate_domain(Some("starkware")).is_err());
    // Hyphen at start or end of label
    assert!(CompanyService::validate_domain(Some("-starkware.co")).is_err());
    assert!(CompanyService::validate_domain(Some("starkware-.co")).is_err());
    // Single-char TLD
    assert!(CompanyService::validate_domain(Some("starkware.c")).is_err());
    // Numeric TLD
    assert!(CompanyService::validate_domain(Some("starkware.123")).is_err());
}

// =========================================================================
// 4. HTTP POST /companies INTEGRATION TESTS
// =========================================================================

#[tokio::test]
async fn test_09_create_company_success() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let req = CreateCompanyRequest {
        name: "Nethermind".to_string(),
        domain: Some("nethermind.io".to_string()),
        description: Some("Leading blockchain research and engineering".to_string()),
    };

    let resp = server.post("/companies").json(&req).await;
    resp.assert_status(StatusCode::CREATED);

    let body: CompanyResponse = resp.json();
    assert_eq!(body.name, "Nethermind");
    assert_eq!(body.slug, "nethermind");
    assert_eq!(body.domain.as_deref(), Some("nethermind.io"));
    assert_eq!(
        body.description.as_deref(),
        Some("Leading blockchain research and engineering")
    );
}

#[tokio::test]
async fn test_10_create_company_validation_error() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();

    // Name too short
    let resp = server
        .post("/companies")
        .json(&CreateCompanyRequest {
            name: "a".to_string(),
            domain: None,
            description: None,
        })
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    // Invalid domain
    let resp = server
        .post("/companies")
        .json(&CreateCompanyRequest {
            name: "Invalid Domain Corp".to_string(),
            domain: Some("https://invalid-url.com".to_string()),
            description: None,
        })
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_11_create_company_domain_conflict_409() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let req1 = CreateCompanyRequest {
        name: "Starknet Foundation".to_string(),
        domain: Some("starknet.org".to_string()),
        description: None,
    };
    let resp1 = server.post("/companies").json(&req1).await;
    resp1.assert_status(StatusCode::CREATED);

    // Duplicate domain
    let req2 = CreateCompanyRequest {
        name: "Starknet Alternate".to_string(),
        domain: Some("starknet.org".to_string()),
        description: None,
    };
    let resp2 = server.post("/companies").json(&req2).await;
    resp2.assert_status(StatusCode::CONFLICT);
}

// =========================================================================
// 5. HTTP GET /companies/{slug} INTEGRATION TESTS
// =========================================================================

#[tokio::test]
async fn test_12_get_company_by_slug_success() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let created = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Kakarot ZkEVM".to_string(),
            domain: Some("kakarot.org".to_string()),
            description: Some("zkEVM built on Cairo".to_string()),
        })
        .unwrap();

    let resp = server.get(&format!("/companies/{}", created.slug)).await;
    resp.assert_status(StatusCode::OK);

    let body: CompanyResponse = resp.json();
    assert_eq!(body.id, created.id);
    assert_eq!(body.slug, "kakarot-zkevm");
    assert_eq!(body.name, "Kakarot ZkEVM");
}

#[tokio::test]
async fn test_13_get_company_not_found_404() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let resp = server.get("/companies/non-existent-company-slug").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_14_get_company_case_insensitive_slug() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let created = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Madara Sequencer".to_string(),
            domain: None,
            description: None,
        })
        .unwrap();

    let resp = server
        .get(&format!("/companies/{}", created.slug.to_uppercase()))
        .await;
    resp.assert_status(StatusCode::OK);

    let body: CompanyResponse = resp.json();
    assert_eq!(body.id, created.id);
}

// =========================================================================
// 6. HTTP PUT /companies/{slug} ADMIN-GUARDED TESTS
// =========================================================================

#[tokio::test]
async fn test_15_update_company_admin_success() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let created = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Briq Construction".to_string(),
            domain: Some("briq.construction".to_string()),
            description: Some("NFT building blocks".to_string()),
        })
        .unwrap();

    let token = create_auth_token(ADMIN_WALLET);

    let update_req = UpdateCompanyRequest {
        name: Some("Briq NFT Studio".to_string()),
        domain: Some("briq.art".to_string()),
        description: Some("Voxel art and composable NFTs on Starknet".to_string()),
    };

    let resp = server
        .put(&format!("/companies/{}", created.slug))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&update_req)
        .await;
    resp.assert_status(StatusCode::OK);

    let updated: CompanyResponse = resp.json();
    assert_eq!(updated.name, "Briq NFT Studio");
    assert_eq!(updated.domain.as_deref(), Some("briq.art"));
    assert_eq!(
        updated.description.as_deref(),
        Some("Voxel art and composable NFTs on Starknet")
    );
    assert_eq!(updated.slug, created.slug); // Slug remains constant
}

#[tokio::test]
async fn test_16_update_company_non_admin_forbidden_403() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let created = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Avnu Dex".to_string(),
            domain: Some("avnu.fi".to_string()),
            description: None,
        })
        .unwrap();

    let token = create_auth_token(NON_ADMIN_WALLET);

    let resp = server
        .put(&format!("/companies/{}", created.slug))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&UpdateCompanyRequest {
            name: Some("Hacked Avnu".to_string()),
            domain: None,
            description: None,
        })
        .await;
    resp.assert_status(StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_17_update_company_missing_auth_401() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let created = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Ekubo Protocol".to_string(),
            domain: Some("ekubo.org".to_string()),
            description: None,
        })
        .unwrap();

    let resp = server
        .put(&format!("/companies/{}", created.slug))
        .json(&UpdateCompanyRequest {
            name: Some("Updated Ekubo".to_string()),
            domain: None,
            description: None,
        })
        .await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_18_update_company_domain_conflict_409() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Braavos Wallet".to_string(),
            domain: Some("braavos.app".to_string()),
            description: None,
        })
        .unwrap();

    let company2 = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Cartridge Controller".to_string(),
            domain: Some("cartridge.gg".to_string()),
            description: None,
        })
        .unwrap();

    let token = create_auth_token(ADMIN_WALLET);

    // Attempt to update Cartridge's domain to braavos.app
    let resp = server
        .put(&format!("/companies/{}", company2.slug))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&UpdateCompanyRequest {
            name: None,
            domain: Some("braavos.app".to_string()),
            description: None,
        })
        .await;
    resp.assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_19_update_company_same_domain_idempotent() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    let company = COMPANY_SERVICE
        .create_company(CreateCompanyRequest {
            name: "Pragma Oracle".to_string(),
            domain: Some("pragma.build".to_string()),
            description: None,
        })
        .unwrap();

    let token = create_auth_token(ADMIN_WALLET);

    // Re-applying the exact same domain should succeed idempotently
    let resp = server
        .put(&format!("/companies/{}", company.slug))
        .add_header("Authorization", format!("Bearer {}", token))
        .json(&UpdateCompanyRequest {
            name: Some("Pragma Decentralized Oracle".to_string()),
            domain: Some("pragma.build".to_string()),
            description: None,
        })
        .await;
    resp.assert_status(StatusCode::OK);

    let updated: CompanyResponse = resp.json();
    assert_eq!(updated.name, "Pragma Decentralized Oracle");
    assert_eq!(updated.domain.as_deref(), Some("pragma.build"));
}

// =========================================================================
// 7. HTTP GET /companies LIST & PAGINATION TESTS
// =========================================================================

#[tokio::test]
async fn test_20_list_companies_pagination() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    COMPANY_SERVICE.reset();

    for i in 1..=5 {
        COMPANY_SERVICE
            .create_company(CreateCompanyRequest {
                name: format!("Company {}", i),
                domain: Some(format!("comp{}.io", i)),
                description: None,
            })
            .unwrap();
    }

    // Page 1: limit 2, offset 0
    let resp1 = server.get("/companies?limit=2&offset=0").await;
    resp1.assert_status(StatusCode::OK);
    let list1: CompanyListResponse = resp1.json();
    assert_eq!(list1.total, 5);
    assert_eq!(list1.items.len(), 2);
    assert_eq!(list1.limit, 2);
    assert_eq!(list1.offset, 0);

    // Page 2: limit 2, offset 2
    let resp2 = server.get("/companies?limit=2&offset=2").await;
    resp2.assert_status(StatusCode::OK);
    let list2: CompanyListResponse = resp2.json();
    assert_eq!(list2.items.len(), 2);
    assert_eq!(list2.offset, 2);

    // Page 3: limit 2, offset 4
    let resp3 = server.get("/companies?limit=2&offset=4").await;
    resp3.assert_status(StatusCode::OK);
    let list3: CompanyListResponse = resp3.json();
    assert_eq!(list3.items.len(), 1);
}

// =========================================================================
// 8. OPENAPI SPEC DOCS VERIFICATION
// =========================================================================

#[tokio::test]
async fn test_21_openapi_spec_contains_company_endpoints() {
    let _guard = TEST_LOCK.lock().await;
    let server = setup_test_app();
    let resp = server.get("/api-docs/openapi.json").await;
    resp.assert_status(StatusCode::OK);

    let spec: serde_json::Value = resp.json();
    let paths = spec.get("paths").expect("paths in openapi spec");

    assert!(paths.get("/companies").is_some());
    assert!(paths.get("/companies/{slug}").is_some());
}

// =========================================================================
// 9. CONCURRENCY & STRESS TEST
// =========================================================================

#[test]
fn test_22_concurrent_slug_collisions() {
    let _guard = TEST_LOCK.blocking_lock();
    COMPANY_SERVICE.reset();

    // Spawn multiple threads creating companies with identical name
    let mut handles = Vec::new();
    for _ in 0..20 {
        let handle = std::thread::spawn(|| {
            COMPANY_SERVICE.create_company(CreateCompanyRequest {
                name: "Concurrent Starknet Org".to_string(),
                domain: None,
                description: None,
            })
        });
        handles.push(handle);
    }

    let mut slugs = std::collections::HashSet::new();
    for handle in handles {
        let res = handle.join().unwrap();
        assert!(res.is_ok());
        let comp = res.unwrap();
        assert!(slugs.insert(comp.slug)); // Every slug must be unique!
    }

    assert_eq!(slugs.len(), 20);
}
