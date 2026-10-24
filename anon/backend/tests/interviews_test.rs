use uuid::Uuid;

use backend::{
    models::interview::{
        CreateInterviewRequest, InterviewQuestion, OverallExperience,
    },
    services::interview_service::{
        InterviewError, InterviewService, MAX_PROCESS_SUMMARY_LEN, MAX_ROLE_TITLE_LEN,
        canonical_wallet,
    },
};

const USER_ALICE: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";
const USER_BOB: &str =
    "0x0222222222222222222222222222222222222222222222222222222222222222";

fn setup_service() -> InterviewService {
    InterviewService::new()
}

fn sample_request(company: &str, role: &str, diff: u8) -> CreateInterviewRequest {
    CreateInterviewRequest {
        company_slug: company.to_string(),
        role_title: role.to_string(),
        location: Some("Remote, Global".to_string()),
        experience_level: Some("Senior".to_string()),
        overall_experience: OverallExperience::Positive,
        difficulty: diff,
        process_summary: "Initial recruiter screen, followed by technical deep dive and take-home project presentation."
            .to_string(),
        questions: vec![
            InterviewQuestion {
                question: "Explain the differences between optimistic and zk-rollups.".to_string(),
                category: Some("technical".to_string()),
            },
            InterviewQuestion {
                question: "How do you handle disagreements on system architecture?".to_string(),
                category: Some("behavioral".to_string()),
            },
        ],
        timeframe: Some("3 weeks".to_string()),
    }
}

#[tokio::test]
async fn test_01_create_interview_happy_path() {
    let service = setup_service();
    let req = sample_request("starkware", "Senior Rust Engineer", 4);
    let review = service
        .create_interview(USER_ALICE, req)
        .expect("interview creation should succeed");

    assert_eq!(review.company_slug, "starkware");
    assert_eq!(review.author_wallet, canonical_wallet(USER_ALICE));
    assert_eq!(review.role_title, "Senior Rust Engineer");
    assert_eq!(review.difficulty, 4);
    assert_eq!(review.overall_experience, OverallExperience::Positive);
    assert_eq!(review.questions.len(), 2);
    assert!(review.timeframe.is_some());
}

#[tokio::test]
async fn test_02_create_interview_with_optional_fields_none() {
    let service = setup_service();
    let req = CreateInterviewRequest {
        company_slug: "argent".to_string(),
        role_title: "Core Dev".to_string(),
        location: None,
        experience_level: None,
        overall_experience: OverallExperience::Neutral,
        difficulty: 3,
        process_summary: "Standard 3 round interview process without complications.".to_string(),
        questions: vec![],
        timeframe: None,
    };
    let review = service
        .create_interview(USER_BOB, req)
        .expect("creation should succeed");

    assert_eq!(review.company_slug, "argent");
    assert_eq!(review.overall_experience, OverallExperience::Neutral);
    assert!(review.location.is_none());
    assert!(review.experience_level.is_none());
    assert!(review.questions.is_empty());
    assert!(review.timeframe.is_none());
}

#[tokio::test]
async fn test_03_create_interview_with_multiple_questions() {
    let service = setup_service();
    let mut req = sample_request("braavos", "Fullstack Cairo Engineer", 3);
    req.questions.push(InterviewQuestion {
        question: "Design a decentralized account abstraction wallet.".to_string(),
        category: Some("system-design".to_string()),
    });

    let review = service
        .create_interview(USER_ALICE, req)
        .expect("creation should succeed");
    assert_eq!(review.questions.len(), 3);
    assert_eq!(
        review.questions[2].category.as_deref(),
        Some("system-design")
    );
}

#[tokio::test]
async fn test_04_create_interview_validation_difficulty_too_low() {
    let service = setup_service();
    let req = sample_request("starkware", "Rust Engineer", 0);
    let err = service.create_interview(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_05_create_interview_validation_difficulty_too_high() {
    let service = setup_service();
    let req = sample_request("starkware", "Rust Engineer", 6);
    let err = service.create_interview(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_06_create_interview_validation_difficulty_bounds_1_and_5_ok() {
    let service = setup_service();
    let req1 = sample_request("starkware", "Junior Dev", 1);
    assert!(service.create_interview(USER_ALICE, req1).is_ok());

    let req5 = sample_request("starkware", "Lead Architect", 5);
    assert!(service.create_interview(USER_BOB, req5).is_ok());
}

#[tokio::test]
async fn test_07_create_interview_validation_empty_company_slug() {
    let service = setup_service();
    let req = sample_request("   ", "Engineer", 3);
    let err = service.create_interview(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_08_create_interview_validation_empty_wallet() {
    let service = setup_service();
    let req = sample_request("starkware", "Engineer", 3);
    let err = service.create_interview("   ", req).unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_09_create_interview_validation_empty_role_title() {
    let service = setup_service();
    let req = sample_request("starkware", " ", 3);
    let err = service.create_interview(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_10_create_interview_validation_role_title_too_long() {
    let service = setup_service();
    let long_title = "A".repeat(MAX_ROLE_TITLE_LEN + 1);
    let req = sample_request("starkware", &long_title, 3);
    let err = service.create_interview(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_11_create_interview_validation_process_summary_too_short() {
    let service = setup_service();
    let mut req = sample_request("starkware", "Engineer", 3);
    req.process_summary = "Too short".to_string(); // 9 chars < 10
    let err = service.create_interview(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_12_create_interview_validation_process_summary_too_long() {
    let service = setup_service();
    let mut req = sample_request("starkware", "Engineer", 3);
    req.process_summary = "A".repeat(MAX_PROCESS_SUMMARY_LEN + 1);
    let err = service.create_interview(USER_ALICE, req).unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_13_create_interview_sanitizes_xss_in_summary() {
    let service = setup_service();
    let mut req = sample_request("starkware", "Engineer", 3);
    req.process_summary =
        "Process included <script>alert('xss')</script> and javascript:void(0) challenges."
            .to_string();
    let review = service.create_interview(USER_ALICE, req).unwrap();

    assert!(!review.process_summary.contains("<script>"));
    assert!(review.process_summary.contains("&lt;script&gt;"));
    assert!(!review.process_summary.contains("javascript:"));
}

#[tokio::test]
async fn test_14_create_interview_sanitizes_xss_in_questions() {
    let service = setup_service();
    let mut req = sample_request("starkware", "Engineer", 3);
    req.questions = vec![InterviewQuestion {
        question: "How do you prevent <script>alert(1)</script> attacks?".to_string(),
        category: Some("SECURITY".to_string()),
    }];
    let review = service.create_interview(USER_ALICE, req).unwrap();

    assert!(!review.questions[0].question.contains("<script>"));
    assert!(review.questions[0].question.contains("&lt;script&gt;"));
    assert_eq!(review.questions[0].category.as_deref(), Some("security"));
}

#[tokio::test]
async fn test_15_get_interview_happy_path() {
    let service = setup_service();
    let req = sample_request("starkware", "Principal Engineer", 4);
    let created = service.create_interview(USER_ALICE, req).unwrap();

    let fetched = service
        .get_interview(created.id)
        .expect("should find created interview");
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.role_title, "Principal Engineer");
}

#[tokio::test]
async fn test_16_get_interview_not_found() {
    let service = setup_service();
    let random_id = Uuid::new_v4();
    let err = service.get_interview(random_id).unwrap_err();
    assert!(matches!(err, InterviewError::NotFound(_)));
}

#[tokio::test]
async fn test_17_list_company_interviews_empty() {
    let service = setup_service();
    let res = service
        .list_company_interviews("nonexistent-company", None, None)
        .unwrap();
    assert_eq!(res.items.len(), 0);
    assert_eq!(res.total, 0);
    assert!(!res.has_more);
    assert!(res.next_cursor.is_none());
}

#[tokio::test]
async fn test_18_list_company_interviews_single() {
    let service = setup_service();
    let req = sample_request("avnu", "Frontend Lead", 3);
    service.create_interview(USER_ALICE, req).unwrap();

    let res = service.list_company_interviews("avnu", None, None).unwrap();
    assert_eq!(res.total, 1);
    assert_eq!(res.items.len(), 1);
    assert_eq!(res.items[0].role_title, "Frontend Lead");
    assert!(!res.has_more);
}

#[tokio::test]
async fn test_19_list_company_interviews_multiple_sorted_by_created_desc() {
    let service = setup_service();
    for i in 1..=5 {
        let req = sample_request("ekubo", &format!("Dev {}", i), 3);
        service.create_interview(USER_ALICE, req).unwrap();
    }

    let res = service
        .list_company_interviews("ekubo", Some(10), None)
        .unwrap();
    assert_eq!(res.total, 5);
    assert_eq!(res.items.len(), 5);
    // Verify descending order
    for w in res.items.windows(2) {
        assert!(w[0].created_at >= w[1].created_at);
    }
}

#[tokio::test]
async fn test_20_list_company_interviews_filter_by_company_isolation() {
    let service = setup_service();
    service
        .create_interview(
            USER_ALICE,
            sample_request("starkware", "Compiler Engineer", 5),
        )
        .unwrap();
    service
        .create_interview(USER_BOB, sample_request("argent", "Mobile Engineer", 3))
        .unwrap();

    let stark_res = service
        .list_company_interviews("starkware", None, None)
        .unwrap();
    assert_eq!(stark_res.total, 1);
    assert_eq!(stark_res.items[0].company_slug, "starkware");

    let argent_res = service
        .list_company_interviews("argent", None, None)
        .unwrap();
    assert_eq!(argent_res.total, 1);
    assert_eq!(argent_res.items[0].company_slug, "argent");
}

#[tokio::test]
async fn test_21_list_company_interviews_company_slug_case_insensitivity() {
    let service = setup_service();
    service
        .create_interview(
            USER_ALICE,
            sample_request("STARKWARE", "Security Engineer", 4),
        )
        .unwrap();

    let res_lower = service
        .list_company_interviews("starkware", None, None)
        .unwrap();
    assert_eq!(res_lower.total, 1);

    let res_upper = service
        .list_company_interviews("StarkWare", None, None)
        .unwrap();
    assert_eq!(res_upper.total, 1);
}

#[tokio::test]
async fn test_22_list_company_interviews_cursor_pagination_first_page() {
    let service = setup_service();
    for i in 1..=5 {
        let req = sample_request("zklend", &format!("Engineer {}", i), 3);
        service.create_interview(USER_ALICE, req).unwrap();
    }

    let page1 = service
        .list_company_interviews("zklend", Some(2), None)
        .unwrap();
    assert_eq!(page1.items.len(), 2);
    assert_eq!(page1.total, 5);
    assert!(page1.has_more);
    assert!(page1.next_cursor.is_some());
}

#[tokio::test]
async fn test_23_list_company_interviews_cursor_pagination_second_page() {
    let service = setup_service();
    for i in 1..=5 {
        let req = sample_request("zklend", &format!("Engineer {}", i), 3);
        service.create_interview(USER_ALICE, req).unwrap();
    }

    let page1 = service
        .list_company_interviews("zklend", Some(2), None)
        .unwrap();
    let cursor = page1.next_cursor.unwrap();

    let page2 = service
        .list_company_interviews("zklend", Some(2), Some(cursor))
        .unwrap();
    assert_eq!(page2.items.len(), 2);
    assert_eq!(page2.total, 5);
    assert!(page2.has_more);

    // Make sure elements from page1 and page2 do not overlap
    let page1_ids: Vec<Uuid> = page1.items.iter().map(|i| i.id).collect();
    for item in &page2.items {
        assert!(!page1_ids.contains(&item.id));
    }
}

#[tokio::test]
async fn test_24_list_company_interviews_cursor_pagination_invalid_cursor() {
    let service = setup_service();
    let err = service
        .list_company_interviews("zklend", Some(2), Some("not-a-valid-uuid".to_string()))
        .unwrap_err();
    assert!(matches!(err, InterviewError::BadRequest(_)));
}

#[tokio::test]
async fn test_25_list_company_interviews_max_limit_clamping() {
    let service = setup_service();
    for i in 1..=60 {
        let req = sample_request("nostra", &format!("Role {}", i), 3);
        service.create_interview(USER_ALICE, req).unwrap();
    }

    // Request limit of 100, should be capped at MAX_LIMIT (50)
    let res = service
        .list_company_interviews("nostra", Some(100), None)
        .unwrap();
    assert_eq!(res.items.len(), 50);
    assert_eq!(res.total, 60);
    assert!(res.has_more);
}

#[tokio::test]
async fn test_26_list_company_interviews_has_more_flag_false_on_last_page() {
    let service = setup_service();
    for i in 1..=3 {
        let req = sample_request("carmine", &format!("Quant {}", i), 4);
        service.create_interview(USER_ALICE, req).unwrap();
    }

    let res = service
        .list_company_interviews("carmine", Some(5), None)
        .unwrap();
    assert_eq!(res.items.len(), 3);
    assert_eq!(res.total, 3);
    assert!(!res.has_more);
    assert!(res.next_cursor.is_none());
}

#[tokio::test]
async fn test_27_overall_experience_enum_serialization_and_display() {
    assert_eq!(
        serde_json::to_string(&OverallExperience::Positive).unwrap(),
        "\"positive\""
    );
    assert_eq!(
        serde_json::to_string(&OverallExperience::Neutral).unwrap(),
        "\"neutral\""
    );
    assert_eq!(
        serde_json::to_string(&OverallExperience::Negative).unwrap(),
        "\"negative\""
    );

    let parsed: OverallExperience = serde_json::from_str("\"positive\"").unwrap();
    assert_eq!(parsed, OverallExperience::Positive);
}
