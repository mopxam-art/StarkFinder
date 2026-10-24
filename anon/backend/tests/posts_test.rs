use chrono::{Duration, Utc};
use uuid::Uuid;

use backend::{
    models::post::{PostStatus, ReviewPost},
    services::post_service::{PostError, PostService},
};

const USER_ALICE: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";

fn setup_service() -> PostService {
    PostService::new()
}

#[tokio::test]
async fn test_01_get_post_by_id_happy_path_200() {
    let service = setup_service();
    let post = service
        .create_post(
            USER_ALICE,
            "starkware",
            "Great engineering leadership and excellent Cairo mentorship.",
            vec!["engineering".to_string(), "culture".to_string()],
            5,
        )
        .expect("post creation should succeed");

    let fetched = service.get_post(post.id).expect("should fetch post by id");
    assert_eq!(fetched.id, post.id);
    assert_eq!(fetched.company_slug, "starkware");
    assert_eq!(fetched.rating, 5);
    assert_eq!(fetched.status, PostStatus::Published);
}

#[tokio::test]
async fn test_02_get_post_by_id_nonexistent_returns_404() {
    let service = setup_service();
    let err = service.get_post(Uuid::new_v4()).unwrap_err();
    assert!(matches!(err, PostError::NotFound(_)));
}

#[tokio::test]
async fn test_03_get_post_by_id_soft_deleted_returns_410_gone() {
    let service = setup_service();
    let post = service
        .create_post(
            USER_ALICE,
            "starkware",
            "Temporary review that will be erased soon.",
            vec![],
            4,
        )
        .unwrap();

    // Soft delete the post
    service.soft_delete_post(post.id).unwrap();

    // Fetch should now return 410 Gone
    let err = service.get_post(post.id).unwrap_err();
    assert!(matches!(err, PostError::Gone(_)));
}

#[tokio::test]
async fn test_04_create_post_validation_empty_slug() {
    let service = setup_service();
    let err = service
        .create_post(USER_ALICE, "   ", "Valid review text here.", vec![], 4)
        .unwrap_err();
    assert!(matches!(err, PostError::BadRequest(_)));
}

#[tokio::test]
async fn test_05_create_post_validation_text_too_short() {
    let service = setup_service();
    let err = service
        .create_post(USER_ALICE, "starkware", "tiny", vec![], 4)
        .unwrap_err();
    assert!(matches!(err, PostError::BadRequest(_)));
}

#[tokio::test]
async fn test_06_create_post_sanitizes_xss_in_text() {
    let service = setup_service();
    let post = service
        .create_post(
            USER_ALICE,
            "starkware",
            "Checking for <script>alert('xss')</script> and javascript:void(0) injection.",
            vec![],
            5,
        )
        .unwrap();

    assert!(!post.text.contains("<script>"));
    assert!(post.text.contains("&lt;script&gt;"));
    assert!(!post.text.contains("javascript:"));
}

#[tokio::test]
async fn test_07_list_company_posts_default_published_only() {
    let service = setup_service();
    let p1 = service
        .create_post(USER_ALICE, "argent", "Review 1 published", vec![], 5)
        .unwrap();
    let mut p2 = service
        .create_post(USER_ALICE, "argent", "Review 2 blocked", vec![], 1)
        .unwrap();
    p2.status = PostStatus::Blocked;
    service.insert_post(p2);

    let feed = service
        .list_company_posts("argent", None, None, None, None, None, None)
        .unwrap();

    assert_eq!(feed.total, 1);
    assert_eq!(feed.items[0].id, p1.id);
}

#[tokio::test]
async fn test_08_list_company_posts_filters_by_tag() {
    let service = setup_service();
    service
        .create_post(
            USER_ALICE,
            "braavos",
            "Review about compensation package.",
            vec!["compensation".to_string()],
            4,
        )
        .unwrap();
    service
        .create_post(
            USER_ALICE,
            "braavos",
            "Review about remote work flexibility.",
            vec!["remote".to_string()],
            5,
        )
        .unwrap();

    let feed = service
        .list_company_posts(
            "braavos",
            None,
            None,
            Some("compensation".to_string()),
            None,
            None,
            None,
        )
        .unwrap();

    assert_eq!(feed.total, 1);
    assert!(feed.items[0].tags.contains(&"compensation".to_string()));
}

#[tokio::test]
async fn test_09_list_company_posts_filters_by_since() {
    let service = setup_service();
    let now = Utc::now();

    let old_post = ReviewPost {
        id: Uuid::new_v4(),
        company_slug: "avnu".to_string(),
        author_wallet: USER_ALICE.to_lowercase(),
        text: "Old review text".to_string(),
        tags: vec![],
        rating: 4,
        status: PostStatus::Published,
        created_at: now - Duration::days(10),
    };
    service.insert_post(old_post.clone());

    let new_post = service
        .create_post(USER_ALICE, "avnu", "Recent review text", vec![], 5)
        .unwrap();

    let feed = service
        .list_company_posts(
            "avnu",
            None,
            None,
            None,
            Some(now - Duration::days(2)),
            None,
            None,
        )
        .unwrap();

    assert_eq!(feed.total, 1);
    assert_eq!(feed.items[0].id, new_post.id);
}

#[tokio::test]
async fn test_10_list_company_posts_filters_by_until() {
    let service = setup_service();
    let now = Utc::now();

    let old_post = ReviewPost {
        id: Uuid::new_v4(),
        company_slug: "avnu".to_string(),
        author_wallet: USER_ALICE.to_lowercase(),
        text: "Old review text".to_string(),
        tags: vec![],
        rating: 4,
        status: PostStatus::Published,
        created_at: now - Duration::days(10),
    };
    service.insert_post(old_post.clone());

    let _new_post = service
        .create_post(USER_ALICE, "avnu", "Recent review text", vec![], 5)
        .unwrap();

    let feed = service
        .list_company_posts(
            "avnu",
            None,
            None,
            None,
            None,
            Some(now - Duration::days(5)),
            None,
        )
        .unwrap();

    assert_eq!(feed.total, 1);
    assert_eq!(feed.items[0].id, old_post.id);
}

#[tokio::test]
async fn test_11_list_company_posts_cursor_pagination_page_1() {
    let service = setup_service();
    for i in 1..=5 {
        service
            .create_post(
                USER_ALICE,
                "ekubo",
                &format!("Review number {}", i),
                vec![],
                4,
            )
            .unwrap();
    }

    let page1 = service
        .list_company_posts("ekubo", Some(2), None, None, None, None, None)
        .unwrap();

    assert_eq!(page1.items.len(), 2);
    assert_eq!(page1.total, 5);
    assert!(page1.has_more);
    assert!(page1.next_cursor.is_some());
}

#[tokio::test]
async fn test_12_list_company_posts_cursor_pagination_page_2() {
    let service = setup_service();
    for i in 1..=5 {
        service
            .create_post(
                USER_ALICE,
                "ekubo",
                &format!("Review number {}", i),
                vec![],
                4,
            )
            .unwrap();
    }

    let page1 = service
        .list_company_posts("ekubo", Some(2), None, None, None, None, None)
        .unwrap();
    let cursor = page1.next_cursor.unwrap();

    let page2 = service
        .list_company_posts("ekubo", Some(2), Some(cursor), None, None, None, None)
        .unwrap();

    assert_eq!(page2.items.len(), 2);
    assert_eq!(page2.total, 5);
    assert!(page2.has_more);

    // Assert disjoint sets
    let p1_ids: Vec<Uuid> = page1.items.iter().map(|p| p.id).collect();
    for item in &page2.items {
        assert!(!p1_ids.contains(&item.id));
    }
}

#[tokio::test]
async fn test_13_list_company_posts_cursor_pagination_invalid_cursor() {
    let service = setup_service();
    let err = service
        .list_company_posts(
            "ekubo",
            Some(2),
            Some("invalid-cursor-format".to_string()),
            None,
            None,
            None,
            None,
        )
        .unwrap_err();

    assert!(matches!(err, PostError::BadRequest(_)));
}

#[tokio::test]
async fn test_14_list_company_posts_empty_feed() {
    let service = setup_service();
    let feed = service
        .list_company_posts("nonexistent-startup", None, None, None, None, None, None)
        .unwrap();

    assert_eq!(feed.total, 0);
    assert!(feed.items.is_empty());
    assert!(!feed.has_more);
    assert!(feed.next_cursor.is_none());
}

#[tokio::test]
async fn test_15_list_company_posts_max_limit_clamping_50() {
    let service = setup_service();
    for i in 1..=60 {
        service
            .create_post(
                USER_ALICE,
                "nostra",
                &format!("Review number {}", i),
                vec![],
                4,
            )
            .unwrap();
    }

    let feed = service
        .list_company_posts("nostra", Some(100), None, None, None, None, None)
        .unwrap();

    assert_eq!(feed.items.len(), 50); // Capped at MAX_LIMIT 50
    assert_eq!(feed.total, 60);
    assert!(feed.has_more);
}

#[tokio::test]
async fn test_16_list_company_posts_ordering_created_at_desc_and_id_desc() {
    let service = setup_service();
    for i in 1..=5 {
        service
            .create_post(
                USER_ALICE,
                "carmine",
                &format!("Review sequence {}", i),
                vec![],
                4,
            )
            .unwrap();
    }

    let feed = service
        .list_company_posts("carmine", Some(10), None, None, None, None, None)
        .unwrap();

    assert_eq!(feed.items.len(), 5);
    for w in feed.items.windows(2) {
        assert!(w[0].created_at >= w[1].created_at);
    }
}

#[tokio::test]
async fn test_17_list_company_posts_hides_blocked_posts_by_default() {
    let service = setup_service();
    let mut blocked = service
        .create_post(USER_ALICE, "zklend", "Inappropriate review", vec![], 1)
        .unwrap();
    blocked.status = PostStatus::Blocked;
    service.insert_post(blocked);

    let feed = service
        .list_company_posts("zklend", None, None, None, None, None, None)
        .unwrap();

    assert_eq!(feed.total, 0);
}

#[tokio::test]
async fn test_18_list_company_posts_hides_deleted_posts_by_default() {
    let service = setup_service();
    let post = service
        .create_post(USER_ALICE, "zklend", "Deleted review", vec![], 3)
        .unwrap();
    service.soft_delete_post(post.id).unwrap();

    let feed = service
        .list_company_posts("zklend", None, None, None, None, None, None)
        .unwrap();

    assert_eq!(feed.total, 0);
}

#[tokio::test]
async fn test_19_list_company_posts_explicit_needs_review_filter() {
    let service = setup_service();
    let mut post = service
        .create_post(USER_ALICE, "zklend", "Pending review moderation", vec![], 3)
        .unwrap();
    post.status = PostStatus::NeedsReview;
    service.insert_post(post.clone());

    let feed = service
        .list_company_posts(
            "zklend",
            None,
            None,
            None,
            None,
            None,
            Some(PostStatus::NeedsReview),
        )
        .unwrap();

    assert_eq!(feed.total, 1);
    assert_eq!(feed.items[0].id, post.id);
}

#[tokio::test]
async fn test_20_case_insensitive_company_slug_feed() {
    let service = setup_service();
    service
        .create_post(USER_ALICE, "STARKWARE", "Review text", vec![], 5)
        .unwrap();

    let feed_lower = service
        .list_company_posts("starkware", None, None, None, None, None, None)
        .unwrap();
    let feed_upper = service
        .list_company_posts("StarkWare", None, None, None, None, None, None)
        .unwrap();

    assert_eq!(feed_lower.total, 1);
    assert_eq!(feed_upper.total, 1);
}

#[tokio::test]
async fn test_21_case_insensitive_tag_filter() {
    let service = setup_service();
    service
        .create_post(
            USER_ALICE,
            "starkware",
            "Review text with tags",
            vec!["engineering".to_string()],
            5,
        )
        .unwrap();

    let feed = service
        .list_company_posts(
            "starkware",
            None,
            None,
            Some("ENGINEERING".to_string()),
            None,
            None,
            None,
        )
        .unwrap();

    assert_eq!(feed.total, 1);
}

#[tokio::test]
async fn test_22_company_isolation_feed() {
    let service = setup_service();
    service
        .create_post(USER_ALICE, "company-one", "Post 1", vec![], 5)
        .unwrap();
    service
        .create_post(USER_ALICE, "company-two", "Post 2", vec![], 4)
        .unwrap();

    let feed1 = service
        .list_company_posts("company-one", None, None, None, None, None, None)
        .unwrap();
    let feed2 = service
        .list_company_posts("company-two", None, None, None, None, None, None)
        .unwrap();

    assert_eq!(feed1.total, 1);
    assert_eq!(feed1.items[0].company_slug, "company-one");
    assert_eq!(feed2.total, 1);
    assert_eq!(feed2.items[0].company_slug, "company-two");
}

#[tokio::test]
async fn test_23_post_status_enum_serde() {
    assert_eq!(
        serde_json::to_string(&PostStatus::Published).unwrap(),
        "\"published\""
    );
    assert_eq!(
        serde_json::to_string(&PostStatus::NeedsReview).unwrap(),
        "\"needs_review\""
    );
    assert_eq!(
        serde_json::to_string(&PostStatus::Blocked).unwrap(),
        "\"blocked\""
    );
    assert_eq!(
        serde_json::to_string(&PostStatus::Deleted).unwrap(),
        "\"deleted\""
    );

    let parsed: PostStatus = serde_json::from_str("\"deleted\"").unwrap();
    assert_eq!(parsed, PostStatus::Deleted);
}

#[tokio::test]
async fn test_24_soft_delete_post_happy_path() {
    let service = setup_service();
    let post = service
        .create_post(USER_ALICE, "starkware", "Post to delete", vec![], 4)
        .unwrap();

    assert!(service.soft_delete_post(post.id).is_ok());
    let err = service.get_post(post.id).unwrap_err();
    assert!(matches!(err, PostError::Gone(_)));
}

#[tokio::test]
async fn test_25_soft_delete_nonexistent_post_404() {
    let service = setup_service();
    let err = service.soft_delete_post(Uuid::new_v4()).unwrap_err();
    assert!(matches!(err, PostError::NotFound(_)));
}

#[tokio::test]
async fn test_26_clear_all_resets_store() {
    let service = setup_service();
    let post = service
        .create_post(USER_ALICE, "starkware", "Sample review", vec![], 5)
        .unwrap();

    service.clear_all();
    let err = service.get_post(post.id).unwrap_err();
    assert!(matches!(err, PostError::NotFound(_)));
}
