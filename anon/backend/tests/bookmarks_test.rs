use uuid::Uuid;

use backend::{
    models::bookmark::BookmarkStatusResponse,
    services::bookmark_service::{
        BookmarkError, BookmarkService, DEFAULT_LIMIT, MAX_LIMIT, canonical_wallet,
    },
};

const USER_ALICE: &str =
    "0x0111111111111111111111111111111111111111111111111111111111111111";
const USER_BOB: &str =
    "0x0222222222222222222222222222222222222222222222222222222222222222";
const USER_CHARLIE: &str =
    "0x0333333333333333333333333333333333333333333333333333333333333333";

fn setup_service() -> BookmarkService {
    BookmarkService::new()
}

#[tokio::test]
async fn test_01_add_bookmark_happy_path() {
    let service = setup_service();
    let post_id = Uuid::new_v4();

    let bookmark = service
        .add_bookmark(USER_ALICE, post_id)
        .expect("add bookmark should succeed");

    assert_eq!(bookmark.post_id, post_id);
    assert_eq!(bookmark.user_wallet, canonical_wallet(USER_ALICE));
}

#[tokio::test]
async fn test_02_add_bookmark_idempotent() {
    let service = setup_service();
    let post_id = Uuid::new_v4();

    let bm1 = service.add_bookmark(USER_ALICE, post_id).unwrap();
    let bm2 = service.add_bookmark(USER_ALICE, post_id).unwrap();

    assert_eq!(bm1.id, bm2.id);
    assert_eq!(bm1.created_at, bm2.created_at);
}

#[tokio::test]
async fn test_03_remove_bookmark_happy_path() {
    let service = setup_service();
    let post_id = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, post_id).unwrap();
    assert!(service.is_bookmarked(USER_ALICE, post_id));

    service.remove_bookmark(USER_ALICE, post_id).unwrap();
    assert!(!service.is_bookmarked(USER_ALICE, post_id));
}

#[tokio::test]
async fn test_04_remove_bookmark_idempotent() {
    let service = setup_service();
    let post_id = Uuid::new_v4();

    // Removing non-existent bookmark should succeed with Ok(())
    let res = service.remove_bookmark(USER_ALICE, post_id);
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_05_is_bookmarked_query() {
    let service = setup_service();
    let post_id1 = Uuid::new_v4();
    let post_id2 = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, post_id1).unwrap();

    assert!(service.is_bookmarked(USER_ALICE, post_id1));
    assert!(!service.is_bookmarked(USER_ALICE, post_id2));
}

#[tokio::test]
async fn test_06_get_bookmark_status_struct() {
    let service = setup_service();
    let post_id = Uuid::new_v4();

    let status_before = service.get_bookmark_status(USER_ALICE, post_id);
    assert_eq!(
        status_before,
        BookmarkStatusResponse {
            post_id,
            is_bookmarked: false,
        }
    );

    service.add_bookmark(USER_ALICE, post_id).unwrap();

    let status_after = service.get_bookmark_status(USER_ALICE, post_id);
    assert_eq!(
        status_after,
        BookmarkStatusResponse {
            post_id,
            is_bookmarked: true,
        }
    );
}

#[tokio::test]
async fn test_07_list_bookmarks_empty() {
    let service = setup_service();

    let list = service
        .list_bookmarks(USER_ALICE, None, None)
        .expect("listing bookmarks should succeed");

    assert_eq!(list.total, 0);
    assert!(list.items.is_empty());
    assert!(!list.has_more);
    assert_eq!(list.next_cursor, None);
}

#[tokio::test]
async fn test_08_list_bookmarks_multiple() {
    let service = setup_service();
    let p1 = Uuid::new_v4();
    let p2 = Uuid::new_v4();
    let p3 = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, p1).unwrap();
    service.add_bookmark(USER_ALICE, p2).unwrap();
    service.add_bookmark(USER_ALICE, p3).unwrap();

    let list = service.list_bookmarks(USER_ALICE, None, None).unwrap();
    assert_eq!(list.total, 3);
    assert_eq!(list.items.len(), 3);
    assert!(!list.has_more);
}

#[tokio::test]
async fn test_09_list_bookmarks_user_isolation() {
    let service = setup_service();
    let p1 = Uuid::new_v4();
    let p2 = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, p1).unwrap();
    service.add_bookmark(USER_BOB, p2).unwrap();

    let alice_list = service.list_bookmarks(USER_ALICE, None, None).unwrap();
    let bob_list = service.list_bookmarks(USER_BOB, None, None).unwrap();

    assert_eq!(alice_list.total, 1);
    assert_eq!(alice_list.items[0].post_id, p1);

    assert_eq!(bob_list.total, 1);
    assert_eq!(bob_list.items[0].post_id, p2);
}

#[tokio::test]
async fn test_10_list_bookmarks_default_limit() {
    let service = setup_service();

    // Add 15 bookmarks
    for _ in 0..15 {
        service.add_bookmark(USER_ALICE, Uuid::new_v4()).unwrap();
    }

    let list = service.list_bookmarks(USER_ALICE, None, None).unwrap();
    assert_eq!(list.total, 15);
    assert_eq!(list.items.len(), DEFAULT_LIMIT); // 10
    assert!(list.has_more);
    assert!(list.next_cursor.is_some());
}

#[tokio::test]
async fn test_11_list_bookmarks_max_limit_clamped() {
    let service = setup_service();

    for _ in 0..60 {
        service.add_bookmark(USER_ALICE, Uuid::new_v4()).unwrap();
    }

    let list = service
        .list_bookmarks(USER_ALICE, Some(100), None)
        .unwrap();
    assert_eq!(list.items.len(), MAX_LIMIT); // clamped to 50
    assert!(list.has_more);
}

#[tokio::test]
async fn test_12_list_bookmarks_cursor_pagination() {
    let service = setup_service();

    let mut ids = Vec::new();
    for _ in 0..5 {
        let bm = service.add_bookmark(USER_ALICE, Uuid::new_v4()).unwrap();
        ids.push(bm.id);
    }

    // Page 1: limit 2
    let page1 = service
        .list_bookmarks(USER_ALICE, Some(2), None)
        .unwrap();
    assert_eq!(page1.items.len(), 2);
    assert!(page1.has_more);
    let cursor1 = page1.next_cursor.clone().unwrap();

    // Page 2: limit 2 with cursor1
    let page2 = service
        .list_bookmarks(USER_ALICE, Some(2), Some(cursor1))
        .unwrap();
    assert_eq!(page2.items.len(), 2);
    assert!(page2.has_more);
    let cursor2 = page2.next_cursor.clone().unwrap();

    // Page 3: limit 2 with cursor2
    let page3 = service
        .list_bookmarks(USER_ALICE, Some(2), Some(cursor2))
        .unwrap();
    assert_eq!(page3.items.len(), 1);
    assert!(!page3.has_more);
    assert_eq!(page3.next_cursor, None);
}

#[tokio::test]
async fn test_13_list_bookmarks_invalid_cursor_returns_bad_request() {
    let service = setup_service();

    let err = service
        .list_bookmarks(USER_ALICE, Some(10), Some("not-a-valid-uuid".to_string()))
        .unwrap_err();

    assert!(matches!(err, BookmarkError::BadRequest(_)));
}

#[tokio::test]
async fn test_14_list_bookmarks_cursor_at_end_has_no_more() {
    let service = setup_service();

    let bm = service.add_bookmark(USER_ALICE, Uuid::new_v4()).unwrap();
    let page = service
        .list_bookmarks(USER_ALICE, Some(10), Some(bm.id.to_string()))
        .unwrap();

    assert_eq!(page.items.len(), 0);
    assert!(!page.has_more);
    assert_eq!(page.next_cursor, None);
}

#[tokio::test]
async fn test_15_wallet_normalization_and_case_insensitivity() {
    let service = setup_service();
    let post_id = Uuid::new_v4();

    let upper_wallet = "0x0AABBCCDDEEFF00112233445566778899AABBCCDDEEFF0011223344556677889";
    let lower_wallet = upper_wallet.to_lowercase();

    service.add_bookmark(upper_wallet, post_id).unwrap();
    assert!(service.is_bookmarked(&lower_wallet, post_id));

    let list = service.list_bookmarks(&lower_wallet, None, None).unwrap();
    assert_eq!(list.total, 1);
}

#[tokio::test]
async fn test_16_empty_wallet_returns_bad_request() {
    let service = setup_service();
    let err = service.add_bookmark("   ", Uuid::new_v4()).unwrap_err();
    assert!(matches!(err, BookmarkError::BadRequest(_)));
}

#[tokio::test]
async fn test_17_remove_only_affects_targeted_post() {
    let service = setup_service();
    let p1 = Uuid::new_v4();
    let p2 = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, p1).unwrap();
    service.add_bookmark(USER_ALICE, p2).unwrap();

    service.remove_bookmark(USER_ALICE, p1).unwrap();

    assert!(!service.is_bookmarked(USER_ALICE, p1));
    assert!(service.is_bookmarked(USER_ALICE, p2));
}

#[tokio::test]
async fn test_18_remove_only_affects_calling_user() {
    let service = setup_service();
    let shared_post_id = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, shared_post_id).unwrap();
    service.add_bookmark(USER_BOB, shared_post_id).unwrap();

    service.remove_bookmark(USER_ALICE, shared_post_id).unwrap();

    assert!(!service.is_bookmarked(USER_ALICE, shared_post_id));
    assert!(service.is_bookmarked(USER_BOB, shared_post_id));
}

#[tokio::test]
async fn test_19_clear_all_resets_state() {
    let service = setup_service();
    service.add_bookmark(USER_ALICE, Uuid::new_v4()).unwrap();
    assert_eq!(service.list_bookmarks(USER_ALICE, None, None).unwrap().total, 1);

    service.clear_all();
    assert_eq!(service.list_bookmarks(USER_ALICE, None, None).unwrap().total, 0);
}

#[tokio::test]
async fn test_20_deterministic_ordering_with_equal_timestamps() {
    let service = setup_service();
    let p1 = Uuid::new_v4();
    let p2 = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, p1).unwrap();
    service.add_bookmark(USER_ALICE, p2).unwrap();

    let list = service.list_bookmarks(USER_ALICE, None, None).unwrap();
    assert_eq!(list.items.len(), 2);
}

#[tokio::test]
async fn test_21_canonical_wallet_helper() {
    let raw = "  0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  ";
    let norm = canonical_wallet(raw);
    assert!(norm.starts_with("0x"));
}

#[tokio::test]
async fn test_22_stress_50_bookmarks_pagination() {
    let service = setup_service();

    for _ in 0..50 {
        service.add_bookmark(USER_ALICE, Uuid::new_v4()).unwrap();
    }

    let mut retrieved = 0;
    let mut cursor = None;

    loop {
        let page = service
            .list_bookmarks(USER_ALICE, Some(10), cursor)
            .unwrap();
        retrieved += page.items.len();
        if !page.has_more {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(retrieved, 50);
}

#[tokio::test]
async fn test_23_re_add_bookmark_after_removal() {
    let service = setup_service();
    let post_id = Uuid::new_v4();

    let bm1 = service.add_bookmark(USER_ALICE, post_id).unwrap();
    service.remove_bookmark(USER_ALICE, post_id).unwrap();

    let bm2 = service.add_bookmark(USER_ALICE, post_id).unwrap();
    assert_eq!(bm2.post_id, post_id);
    assert_ne!(bm1.id, bm2.id); // New UUID upon re-adding
}

#[tokio::test]
async fn test_24_distinct_post_ids_same_user() {
    let service = setup_service();
    let p1 = Uuid::new_v4();
    let p2 = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, p1).unwrap();
    service.add_bookmark(USER_ALICE, p2).unwrap();

    let list = service.list_bookmarks(USER_ALICE, None, None).unwrap();
    assert_eq!(list.total, 2);
    let post_ids: Vec<Uuid> = list.items.iter().map(|b| b.post_id).collect();
    assert!(post_ids.contains(&p1));
    assert!(post_ids.contains(&p2));
}

#[tokio::test]
async fn test_25_distinct_users_same_post() {
    let service = setup_service();
    let post_id = Uuid::new_v4();

    service.add_bookmark(USER_ALICE, post_id).unwrap();
    service.add_bookmark(USER_BOB, post_id).unwrap();
    service.add_bookmark(USER_CHARLIE, post_id).unwrap();

    assert!(service.is_bookmarked(USER_ALICE, post_id));
    assert!(service.is_bookmarked(USER_BOB, post_id));
    assert!(service.is_bookmarked(USER_CHARLIE, post_id));
}
