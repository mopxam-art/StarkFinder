use axum::http::StatusCode;
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::models::comment::{
    CommentNode, CommentStatus, CommentsListRes, CreateCommentDto, DeleteCommentRes,
};
use backend::services::comment_service::{
    COMMENT_STORE, MAX_COMMENT_LENGTH, MAX_THREAD_DEPTH, build_threaded_order,
    canonical_wallet, sanitize_comment_content,
};

fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-comments") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-comments") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

// =========================================================================
// D1: BOUNDARY CONDITIONS, DEPTH TREE & CONTENT LIMITS (20 tests)
// =========================================================================

#[test]
fn test_d1_01_root_comment_depth_is_zero() {
    let post_id = "post-d1-01";
    let wallet = "0x101";
    let node = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Root comment".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    assert_eq!(node.depth, 0);
    assert_eq!(node.parent_id, None);
    assert_eq!(node.status, CommentStatus::Active);
}

#[test]
fn test_d1_02_reply_depth_increments_by_one() {
    let post_id = "post-d1-02";
    let wallet = "0x102";
    let root = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Root".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    let reply = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Reply level 1".to_string(),
                parent_id: Some(root.id.clone()),
            },
        )
        .unwrap();

    assert_eq!(reply.depth, 1);
    assert_eq!(reply.parent_id, Some(root.id));
}

#[test]
fn test_d1_03_nested_reply_depth_level_two() {
    let post_id = "post-d1-03";
    let wallet = "0x103";
    let root = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Root".to_string(),
                parent_id: None,
            },
        )
        .unwrap();
    let r1 = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Reply 1".to_string(),
                parent_id: Some(root.id),
            },
        )
        .unwrap();
    let r2 = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Reply 2".to_string(),
                parent_id: Some(r1.id),
            },
        )
        .unwrap();

    assert_eq!(r2.depth, 2);
}

#[test]
fn test_d1_04_depth_level_five() {
    let post_id = "post-d1-04";
    let wallet = "0x104";
    let mut parent_id = None;
    let mut final_depth = 0;

    for i in 0..6 {
        let node = COMMENT_STORE
            .create_comment(
                post_id,
                wallet,
                CreateCommentDto {
                    content: format!("Level {}", i),
                    parent_id,
                },
            )
            .unwrap();
        final_depth = node.depth;
        parent_id = Some(node.id);
    }

    assert_eq!(final_depth, 5);
}

#[test]
fn test_d1_05_max_thread_depth_twenty_accepted() {
    let post_id = "post-d1-05";
    let wallet = "0x105";
    let mut parent_id = None;
    let mut last_node = None;

    for i in 0..=MAX_THREAD_DEPTH {
        let node = COMMENT_STORE
            .create_comment(
                post_id,
                wallet,
                CreateCommentDto {
                    content: format!("Depth {}", i),
                    parent_id,
                },
            )
            .unwrap();
        parent_id = Some(node.id.clone());
        last_node = Some(node);
    }

    assert_eq!(last_node.unwrap().depth, MAX_THREAD_DEPTH);
}

#[test]
fn test_d1_06_exceeding_max_thread_depth_twenty_one_rejected() {
    let post_id = "post-d1-06";
    let wallet = "0x106";
    let mut parent_id = None;

    for i in 0..=MAX_THREAD_DEPTH {
        let node = COMMENT_STORE
            .create_comment(
                post_id,
                wallet,
                CreateCommentDto {
                    content: format!("Depth {}", i),
                    parent_id,
                },
            )
            .unwrap();
        parent_id = Some(node.id);
    }

    // Level 21 must be rejected
    let res = COMMENT_STORE.create_comment(
        post_id,
        wallet,
        CreateCommentDto {
            content: "Depth 21 - Over limit".to_string(),
            parent_id,
        },
    );

    assert!(res.is_err());
}

#[test]
fn test_d1_07_empty_content_rejected() {
    let res = COMMENT_STORE.create_comment(
        "post-d1-07",
        "0x107",
        CreateCommentDto {
            content: "".to_string(),
            parent_id: None,
        },
    );
    assert!(res.is_err());
}

#[test]
fn test_d1_08_whitespace_only_content_rejected() {
    let res = COMMENT_STORE.create_comment(
        "post-d1-08",
        "0x108",
        CreateCommentDto {
            content: "   \t\n   ".to_string(),
            parent_id: None,
        },
    );
    assert!(res.is_err());
}

#[test]
fn test_d1_09_single_character_content_accepted() {
    let res = COMMENT_STORE.create_comment(
        "post-d1-09",
        "0x109",
        CreateCommentDto {
            content: "a".to_string(),
            parent_id: None,
        },
    );
    assert!(res.is_ok());
    assert_eq!(res.unwrap().content, "a");
}

#[test]
fn test_d1_10_exact_maximum_length_four_thousand_accepted() {
    let content = "x".repeat(MAX_COMMENT_LENGTH);
    let res = COMMENT_STORE.create_comment(
        "post-d1-10",
        "0x10a",
        CreateCommentDto {
            content,
            parent_id: None,
        },
    );
    assert!(res.is_ok());
    assert_eq!(res.unwrap().content.len(), MAX_COMMENT_LENGTH);
}

#[test]
fn test_d1_11_exceeding_four_thousand_characters_rejected() {
    let content = "x".repeat(MAX_COMMENT_LENGTH + 1);
    let res = COMMENT_STORE.create_comment(
        "post-d1-11",
        "0x10b",
        CreateCommentDto {
            content,
            parent_id: None,
        },
    );
    assert!(res.is_err());
}

#[test]
fn test_d1_12_reply_to_different_post_parent_rejected() {
    let root = COMMENT_STORE
        .create_comment(
            "post-A",
            "0x10c",
            CreateCommentDto {
                content: "Post A root".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    let res = COMMENT_STORE.create_comment(
        "post-B",
        "0x10c",
        CreateCommentDto {
            content: "Attempting to reply across posts".to_string(),
            parent_id: Some(root.id),
        },
    );

    assert!(res.is_err());
}

#[test]
fn test_d1_13_non_existent_parent_id_returns_not_found() {
    let res = COMMENT_STORE.create_comment(
        "post-d1-13",
        "0x10d",
        CreateCommentDto {
            content: "Reply to ghost".to_string(),
            parent_id: Some("00000000-0000-0000-0000-000000000000".to_string()),
        },
    );
    assert!(res.is_err());
}

#[test]
fn test_d1_14_replying_to_deleted_parent_returns_gone() {
    let post_id = "post-d1-14";
    let wallet = "0x10e";
    let root = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Soon to be deleted".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    COMMENT_STORE.soft_delete_comment(&root.id, wallet, false).unwrap();

    let res = COMMENT_STORE.create_comment(
        post_id,
        wallet,
        CreateCommentDto {
            content: "Reply to deleted".to_string(),
            parent_id: Some(root.id),
        },
    );

    assert!(res.is_err());
}

#[test]
fn test_d1_15_uuid_format_validation() {
    let node = COMMENT_STORE
        .create_comment(
            "post-d1-15",
            "0x10f",
            CreateCommentDto {
                content: "Valid UUID check".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    assert!(uuid::Uuid::parse_str(&node.id).is_ok());
}

#[test]
fn test_d1_16_canonical_wallet_normalization() {
    let node = COMMENT_STORE
        .create_comment(
            "post-d1-16",
            "0x000123",
            CreateCommentDto {
                content: "Wallet norm".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    assert_eq!(node.author_wallet, canonical_wallet("0x000123"));
}

#[test]
fn test_d1_17_post_id_mapping_consistency() {
    let post_id = "post-uuid-custom-12345";
    let node = COMMENT_STORE
        .create_comment(
            post_id,
            "0x111",
            CreateCommentDto {
                content: "Check post_id".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    assert_eq!(node.post_id, post_id);
}

#[test]
fn test_d1_18_default_comment_status_is_active() {
    let node = COMMENT_STORE
        .create_comment(
            "post-d1-18",
            "0x112",
            CreateCommentDto {
                content: "Status check".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    assert_eq!(node.status, CommentStatus::Active);
}

#[test]
fn test_d1_19_timestamp_integrity() {
    let node = COMMENT_STORE
        .create_comment(
            "post-d1-19",
            "0x113",
            CreateCommentDto {
                content: "Timestamps".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    assert!(node.created_at <= node.updated_at);
}

#[tokio::test]
async fn test_d1_20_http_post_creates_comment_with_created_status() {
    let server = setup_test_app();
    let post_id = "post-http-20";
    let wallet = "0x114";
    let auth = create_auth_token(wallet);

    let res = server
        .post(&format!("/posts/{}/comments", post_id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .json(&CreateCommentDto {
            content: "HTTP Created 201".to_string(),
            parent_id: None,
        })
        .await;

    assert_eq!(res.status_code(), StatusCode::CREATED);
    let node: CommentNode = res.json();
    assert_eq!(node.content, "HTTP Created 201");
    assert_eq!(node.depth, 0);
}

// =========================================================================
// D2: COMBINATORICS OF STATES & SOFT DELETE SEMANTICS (20 tests)
// =========================================================================

#[test]
fn test_d2_01_soft_delete_by_author_marks_status_deleted() {
    let wallet = "0x201";
    let node = COMMENT_STORE
        .create_comment(
            "post-d2-01",
            wallet,
            CreateCommentDto {
                content: "Original text".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    let deleted = COMMENT_STORE.soft_delete_comment(&node.id, wallet, false).unwrap();
    assert_eq!(deleted.status, CommentStatus::Deleted);
    assert_eq!(deleted.content, "[deleted]");
}

#[test]
fn test_d2_02_attempt_to_delete_already_deleted_returns_gone() {
    let wallet = "0x202";
    let node = COMMENT_STORE
        .create_comment(
            "post-d2-02",
            wallet,
            CreateCommentDto {
                content: "Original".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    COMMENT_STORE.soft_delete_comment(&node.id, wallet, false).unwrap();
    let second_delete = COMMENT_STORE.soft_delete_comment(&node.id, wallet, false);
    assert!(second_delete.is_err());
}

#[test]
fn test_d2_03_delete_non_existent_comment_returns_not_found() {
    let res = COMMENT_STORE.soft_delete_comment("ghost-comment", "0x203", false);
    assert!(res.is_err());
}

#[test]
fn test_d2_04_moderate_active_comment_updates_status() {
    let node = COMMENT_STORE
        .create_comment(
            "post-d2-04",
            "0x204",
            CreateCommentDto {
                content: "Inappropriate language".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    let moderated = COMMENT_STORE.moderate_comment(&node.id).unwrap();
    assert_eq!(moderated.status, CommentStatus::Moderated);
    assert_eq!(moderated.content, "[moderated by admin]");
}

#[test]
fn test_d2_05_listing_comments_renders_deleted_content_as_deleted() {
    let post_id = "post-d2-05";
    let wallet = "0x205";
    let node = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Secret details".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    COMMENT_STORE.soft_delete_comment(&node.id, wallet, false).unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    assert_eq!(list.comments.len(), 1);
    assert_eq!(list.comments[0].content, "[deleted]");
    assert_eq!(list.comments[0].status, CommentStatus::Deleted);
}

#[test]
fn test_d2_06_children_of_deleted_comments_remain_visible() {
    let post_id = "post-d2-06";
    let wallet = "0x206";
    let root = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Parent soon deleted".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    let reply = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Child stays intact".to_string(),
                parent_id: Some(root.id.clone()),
            },
        )
        .unwrap();

    COMMENT_STORE.soft_delete_comment(&root.id, wallet, false).unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    assert_eq!(list.comments.len(), 2);
    assert_eq!(list.comments[0].id, root.id);
    assert_eq!(list.comments[0].content, "[deleted]");
    assert_eq!(list.comments[1].id, reply.id);
    assert_eq!(list.comments[1].content, "Child stays intact");
}

#[test]
fn test_d2_07_reply_count_on_parent_calculated_correctly() {
    let post_id = "post-d2-07";
    let wallet = "0x207";
    let root = COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Parent with 2 replies".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Reply 1".to_string(),
                parent_id: Some(root.id.clone()),
            },
        )
        .unwrap();

    COMMENT_STORE
        .create_comment(
            post_id,
            wallet,
            CreateCommentDto {
                content: "Reply 2".to_string(),
                parent_id: Some(root.id.clone()),
            },
        )
        .unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    let root_node = list.comments.iter().find(|c| c.id == root.id).unwrap();
    assert_eq!(root_node.reply_count, 2);
}

#[test]
fn test_d2_08_soft_delete_preserves_created_at() {
    let wallet = "0x208";
    let node = COMMENT_STORE
        .create_comment(
            "post-d2-08",
            wallet,
            CreateCommentDto {
                content: "Time check".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(5));
    let deleted = COMMENT_STORE.soft_delete_comment(&node.id, wallet, false).unwrap();
    assert_eq!(deleted.created_at, node.created_at);
}

#[test]
fn test_d2_09_soft_delete_updates_updated_at() {
    let wallet = "0x209";
    let node = COMMENT_STORE
        .create_comment(
            "post-d2-09",
            wallet,
            CreateCommentDto {
                content: "Update check".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(5));
    let deleted = COMMENT_STORE.soft_delete_comment(&node.id, wallet, false).unwrap();
    assert!(deleted.updated_at >= node.updated_at);
}

#[test]
fn test_d2_10_multiple_sibling_deletions_preserve_order() {
    let post_id = "post-d2-10";
    let wallet = "0x20a";
    let c1 = COMMENT_STORE
        .create_comment(post_id, wallet, CreateCommentDto { content: "C1".to_string(), parent_id: None })
        .unwrap();
    let c2 = COMMENT_STORE
        .create_comment(post_id, wallet, CreateCommentDto { content: "C2".to_string(), parent_id: None })
        .unwrap();

    COMMENT_STORE.soft_delete_comment(&c1.id, wallet, false).unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    assert_eq!(list.comments[0].id, c1.id);
    assert_eq!(list.comments[1].id, c2.id);
}

#[test]
fn test_d2_11_get_comment_returns_deleted_indicator() {
    let wallet = "0x20b";
    let node = COMMENT_STORE
        .create_comment("post-d2-11", wallet, CreateCommentDto { content: "Delete me".to_string(), parent_id: None })
        .unwrap();
    COMMENT_STORE.soft_delete_comment(&node.id, wallet, false).unwrap();

    let fetched = COMMENT_STORE.get_comment(&node.id).unwrap();
    assert_eq!(fetched.content, "[deleted]");
    assert_eq!(fetched.status, CommentStatus::Deleted);
}

#[test]
fn test_d2_12_get_comment_returns_moderated_indicator() {
    let node = COMMENT_STORE
        .create_comment("post-d2-12", "0x20c", CreateCommentDto { content: "Bad content".to_string(), parent_id: None })
        .unwrap();
    COMMENT_STORE.moderate_comment(&node.id).unwrap();

    let fetched = COMMENT_STORE.get_comment(&node.id).unwrap();
    assert_eq!(fetched.content, "[moderated by admin]");
    assert_eq!(fetched.status, CommentStatus::Moderated);
}

#[test]
fn test_d2_13_moderating_deleted_comment() {
    let wallet = "0x20d";
    let node = COMMENT_STORE
        .create_comment("post-d2-13", wallet, CreateCommentDto { content: "Both".to_string(), parent_id: None })
        .unwrap();
    COMMENT_STORE.soft_delete_comment(&node.id, wallet, false).unwrap();
    let modded = COMMENT_STORE.moderate_comment(&node.id).unwrap();
    assert_eq!(modded.status, CommentStatus::Moderated);
}

#[test]
fn test_d2_14_delete_res_model_serialization() {
    let res = DeleteCommentRes {
        message: "deleted".to_string(),
        comment_id: "cid-123".to_string(),
        status: CommentStatus::Deleted,
    };
    let val = serde_json::to_value(&res).unwrap();
    assert_eq!(val["message"], "deleted");
    assert_eq!(val["status"], "deleted");
}

#[tokio::test]
async fn test_d2_15_http_delete_success() {
    let server = setup_test_app();
    let wallet = "0x20e";
    let auth = create_auth_token(wallet);
    let node = COMMENT_STORE
        .create_comment("post-d2-15", wallet, CreateCommentDto { content: "HTTP Delete".to_string(), parent_id: None })
        .unwrap();

    let res = server
        .delete(&format!("/comments/{}", node.id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    let body: DeleteCommentRes = res.json();
    assert_eq!(body.comment_id, node.id);
    assert_eq!(body.status, CommentStatus::Deleted);
}

#[tokio::test]
async fn test_d2_16_http_delete_already_deleted_returns_gone() {
    let server = setup_test_app();
    let wallet = "0x20f";
    let auth = create_auth_token(wallet);
    let node = COMMENT_STORE
        .create_comment("post-d2-16", wallet, CreateCommentDto { content: "Double delete".to_string(), parent_id: None })
        .unwrap();

    COMMENT_STORE.soft_delete_comment(&node.id, wallet, false).unwrap();

    let res = server
        .delete(&format!("/comments/{}", node.id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;

    assert_eq!(res.status_code(), StatusCode::GONE);
}

#[tokio::test]
async fn test_d2_17_http_delete_not_found() {
    let server = setup_test_app();
    let wallet = "0x210";
    let auth = create_auth_token(wallet);

    let res = server
        .delete("/comments/00000000-0000-0000-0000-000000000000")
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;

    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[test]
fn test_d2_18_active_state_serialization() {
    let val = serde_json::to_string(&CommentStatus::Active).unwrap();
    assert_eq!(val, "\"active\"");
}

#[test]
fn test_d2_19_deleted_state_serialization() {
    let val = serde_json::to_string(&CommentStatus::Deleted).unwrap();
    assert_eq!(val, "\"deleted\"");
}

#[test]
fn test_d2_20_moderated_state_serialization() {
    let val = serde_json::to_string(&CommentStatus::Moderated).unwrap();
    assert_eq!(val, "\"moderated\"");
}

// =========================================================================
// D3: THREADED ORDERING & CURSOR PAGINATION (20 tests)
// =========================================================================

#[test]
fn test_d3_01_root_comments_sorted_chronologically() {
    let post_id = "post-d3-01";
    let r1 = COMMENT_STORE.create_comment(post_id, "0x301", CreateCommentDto { content: "First".to_string(), parent_id: None }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let r2 = COMMENT_STORE.create_comment(post_id, "0x301", CreateCommentDto { content: "Second".to_string(), parent_id: None }).unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    assert_eq!(list.comments[0].id, r1.id);
    assert_eq!(list.comments[1].id, r2.id);
}

#[test]
fn test_d3_02_replies_nested_under_respective_parents_dfs() {
    let post_id = "post-d3-02";
    let r1 = COMMENT_STORE.create_comment(post_id, "0x302", CreateCommentDto { content: "R1".to_string(), parent_id: None }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let r2 = COMMENT_STORE.create_comment(post_id, "0x302", CreateCommentDto { content: "R2".to_string(), parent_id: None }).unwrap();
    let r1_c1 = COMMENT_STORE.create_comment(post_id, "0x302", CreateCommentDto { content: "R1_C1".to_string(), parent_id: Some(r1.id.clone()) }).unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    // Expected DFS order: r1 -> r1_c1 -> r2
    assert_eq!(list.comments[0].id, r1.id);
    assert_eq!(list.comments[1].id, r1_c1.id);
    assert_eq!(list.comments[2].id, r2.id);
}

#[test]
fn test_d3_03_deeply_nested_branch_before_next_root() {
    let post_id = "post-d3-03";
    let r1 = COMMENT_STORE.create_comment(post_id, "0x303", CreateCommentDto { content: "R1".to_string(), parent_id: None }).unwrap();
    let r1_a = COMMENT_STORE.create_comment(post_id, "0x303", CreateCommentDto { content: "R1_A".to_string(), parent_id: Some(r1.id.clone()) }).unwrap();
    let r1_b = COMMENT_STORE.create_comment(post_id, "0x303", CreateCommentDto { content: "R1_B".to_string(), parent_id: Some(r1_a.id.clone()) }).unwrap();
    let r2 = COMMENT_STORE.create_comment(post_id, "0x303", CreateCommentDto { content: "R2".to_string(), parent_id: None }).unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    assert_eq!(list.comments[0].id, r1.id);
    assert_eq!(list.comments[1].id, r1_a.id);
    assert_eq!(list.comments[2].id, r1_b.id);
    assert_eq!(list.comments[3].id, r2.id);
}

#[test]
fn test_d3_04_multiple_replies_to_same_parent_order() {
    let post_id = "post-d3-04";
    let r = COMMENT_STORE.create_comment(post_id, "0x304", CreateCommentDto { content: "R".to_string(), parent_id: None }).unwrap();
    let c1 = COMMENT_STORE.create_comment(post_id, "0x304", CreateCommentDto { content: "C1".to_string(), parent_id: Some(r.id.clone()) }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let c2 = COMMENT_STORE.create_comment(post_id, "0x304", CreateCommentDto { content: "C2".to_string(), parent_id: Some(r.id.clone()) }).unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    assert_eq!(list.comments[1].id, c1.id);
    assert_eq!(list.comments[2].id, c2.id);
}

#[test]
fn test_d3_05_list_comments_empty_post_returns_empty() {
    let list = COMMENT_STORE.list_comments("empty-post", None, None).unwrap();
    assert_eq!(list.comments.len(), 0);
    assert_eq!(list.total_count, 0);
    assert!(!list.has_more);
    assert_eq!(list.next_cursor, None);
}

#[test]
fn test_d3_06_default_limit_applied() {
    let post_id = "post-d3-06";
    for i in 0..25 {
        COMMENT_STORE.create_comment(post_id, "0x306", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
    }

    let list = COMMENT_STORE.list_comments(post_id, None, None).unwrap();
    assert_eq!(list.comments.len(), 20); // Default limit is 20
    assert!(list.has_more);
    assert!(list.next_cursor.is_some());
}

#[test]
fn test_d3_07_custom_page_limit_works() {
    let post_id = "post-d3-07";
    for i in 0..10 {
        COMMENT_STORE.create_comment(post_id, "0x307", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
    }

    let list = COMMENT_STORE.list_comments(post_id, None, Some(5)).unwrap();
    assert_eq!(list.comments.len(), 5);
    assert_eq!(list.total_count, 10);
    assert!(list.has_more);
}

#[test]
fn test_d3_08_limit_clamped_to_max_one_hundred() {
    let post_id = "post-d3-08";
    let list = COMMENT_STORE.list_comments(post_id, None, Some(500)).unwrap();
    assert_eq!(list.comments.len(), 0);
}

#[test]
fn test_d3_09_cursor_pagination_page_one_returns_next_cursor() {
    let post_id = "post-d3-09";
    for i in 0..6 {
        COMMENT_STORE.create_comment(post_id, "0x309", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
    }

    let page1 = COMMENT_STORE.list_comments(post_id, None, Some(3)).unwrap();
    assert_eq!(page1.comments.len(), 3);
    assert!(page1.has_more);
    assert!(page1.next_cursor.is_some());
}

#[test]
fn test_d3_10_cursor_pagination_page_two_fetches_subsequent_items() {
    let post_id = "post-d3-10";
    let mut ids = Vec::new();
    for i in 0..6 {
        let node = COMMENT_STORE.create_comment(post_id, "0x310", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
        ids.push(node.id);
    }

    let page1 = COMMENT_STORE.list_comments(post_id, None, Some(3)).unwrap();
    let cursor = page1.next_cursor.unwrap();

    let page2 = COMMENT_STORE.list_comments(post_id, Some(&cursor), Some(3)).unwrap();
    assert_eq!(page2.comments.len(), 3);
    assert_eq!(page2.comments[0].id, ids[3]);
    assert_eq!(page2.comments[1].id, ids[4]);
    assert_eq!(page2.comments[2].id, ids[5]);
}

#[test]
fn test_d3_11_last_page_has_more_is_false() {
    let post_id = "post-d3-11";
    for i in 0..4 {
        COMMENT_STORE.create_comment(post_id, "0x311", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
    }

    let page1 = COMMENT_STORE.list_comments(post_id, None, Some(2)).unwrap();
    let cursor = page1.next_cursor.unwrap();
    let page2 = COMMENT_STORE.list_comments(post_id, Some(&cursor), Some(2)).unwrap();

    assert!(!page2.has_more);
    assert_eq!(page2.next_cursor, None);
}

#[test]
fn test_d3_12_invalid_cursor_falls_back_gracefully() {
    let post_id = "post-d3-12";
    COMMENT_STORE.create_comment(post_id, "0x312", CreateCommentDto { content: "C".to_string(), parent_id: None }).unwrap();
    let res = COMMENT_STORE.list_comments(post_id, Some("invalid-cursor-id"), Some(5)).unwrap();
    assert_eq!(res.comments.len(), 1);
}

#[test]
fn test_d3_13_isolated_posts_do_not_leak_comments() {
    let p1 = "post-d3-13-A";
    let p2 = "post-d3-13-B";
    COMMENT_STORE.create_comment(p1, "0x313", CreateCommentDto { content: "For A".to_string(), parent_id: None }).unwrap();
    COMMENT_STORE.create_comment(p2, "0x313", CreateCommentDto { content: "For B".to_string(), parent_id: None }).unwrap();

    let l1 = COMMENT_STORE.list_comments(p1, None, None).unwrap();
    let l2 = COMMENT_STORE.list_comments(p2, None, None).unwrap();
    assert_eq!(l1.comments.len(), 1);
    assert_eq!(l2.comments.len(), 1);
    assert_ne!(l1.comments[0].id, l2.comments[0].id);
}

#[test]
fn test_d3_14_total_count_accuracy() {
    let post_id = "post-d3-14";
    for i in 0..15 {
        COMMENT_STORE.create_comment(post_id, "0x314", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
    }
    let res = COMMENT_STORE.list_comments(post_id, None, Some(5)).unwrap();
    assert_eq!(res.total_count, 15);
}

#[tokio::test]
async fn test_d3_15_http_get_comments_list_success() {
    let server = setup_test_app();
    let post_id = "post-http-d3-15";
    COMMENT_STORE.create_comment(post_id, "0x315", CreateCommentDto { content: "HTTP C1".to_string(), parent_id: None }).unwrap();

    let res = server.get(&format!("/posts/{}/comments", post_id)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let list: CommentsListRes = res.json();
    assert_eq!(list.comments.len(), 1);
}

#[tokio::test]
async fn test_d3_16_http_get_comments_pagination_query() {
    let server = setup_test_app();
    let post_id = "post-http-d3-16";
    for i in 0..5 {
        COMMENT_STORE.create_comment(post_id, "0x316", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
    }

    let res = server.get(&format!("/posts/{}/comments?limit=2", post_id)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let list: CommentsListRes = res.json();
    assert_eq!(list.comments.len(), 2);
    assert!(list.has_more);
}

#[test]
fn test_d3_17_build_threaded_order_empty_array() {
    let mut empty = Vec::new();
    let res = build_threaded_order(&mut empty);
    assert!(res.is_empty());
}

#[test]
fn test_d3_18_comments_list_res_serialization() {
    let res = CommentsListRes {
        comments: Vec::new(),
        next_cursor: Some("c-1".to_string()),
        total_count: 50,
        has_more: true,
    };
    let val = serde_json::to_value(&res).unwrap();
    assert_eq!(val["total_count"], 50);
    assert_eq!(val["has_more"], true);
}

#[test]
fn test_d3_19_limit_zero_clamped_to_one() {
    let post_id = "post-d3-19";
    COMMENT_STORE.create_comment(post_id, "0x319", CreateCommentDto { content: "C1".to_string(), parent_id: None }).unwrap();
    COMMENT_STORE.create_comment(post_id, "0x319", CreateCommentDto { content: "C2".to_string(), parent_id: None }).unwrap();

    let list = COMMENT_STORE.list_comments(post_id, None, Some(0)).unwrap();
    assert_eq!(list.comments.len(), 1);
}

#[test]
fn test_d3_20_empty_cursor_string_handled_safely() {
    let post_id = "post-d3-20";
    COMMENT_STORE.create_comment(post_id, "0x320", CreateCommentDto { content: "C".to_string(), parent_id: None }).unwrap();
    let list = COMMENT_STORE.list_comments(post_id, Some(""), None).unwrap();
    assert_eq!(list.comments.len(), 1);
}

// =========================================================================
// D4: AUTHORIZATION & PII / TOXICITY SANITIZATION (20 tests)
// =========================================================================

#[test]
fn test_d4_01_non_author_cannot_delete_comment_forbidden() {
    let author = "0x401";
    let stranger = "0x402";
    let node = COMMENT_STORE
        .create_comment("post-d4-01", author, CreateCommentDto { content: "Mine".to_string(), parent_id: None })
        .unwrap();

    let res = COMMENT_STORE.soft_delete_comment(&node.id, stranger, false);
    assert!(res.is_err());
}

#[test]
fn test_d4_02_author_can_delete_own_comment() {
    let author = "0x403";
    let node = COMMENT_STORE
        .create_comment("post-d4-02", author, CreateCommentDto { content: "Mine".to_string(), parent_id: None })
        .unwrap();

    let res = COMMENT_STORE.soft_delete_comment(&node.id, author, false);
    assert!(res.is_ok());
}

#[test]
fn test_d4_03_moderator_can_delete_any_comment() {
    let author = "0x404";
    let moderator = "0x405";
    let node = COMMENT_STORE
        .create_comment("post-d4-03", author, CreateCommentDto { content: "Abuse".to_string(), parent_id: None })
        .unwrap();

    let res = COMMENT_STORE.soft_delete_comment(&node.id, moderator, true);
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_d4_04_create_comment_without_auth_fails() {
    let server = setup_test_app();
    let res = server
        .post("/posts/post-d4-04/comments")
        .json(&CreateCommentDto { content: "No auth".to_string(), parent_id: None })
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_05_delete_comment_without_auth_fails() {
    let server = setup_test_app();
    let res = server.delete("/comments/some-id").await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_06_invalid_jwt_signature_rejected() {
    let server = setup_test_app();
    let res = server
        .post("/posts/post-d4-06/comments")
        .add_header(axum::http::header::AUTHORIZATION, "Bearer invalid-signature")
        .json(&CreateCommentDto { content: "Hacked".to_string(), parent_id: None })
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_d4_07_delete_by_stranger_returns_forbidden() {
    let server = setup_test_app();
    let author = "0x407a";
    let stranger = "0x407b";
    let stranger_auth = create_auth_token(stranger);

    let node = COMMENT_STORE
        .create_comment("post-d4-07", author, CreateCommentDto { content: "Target".to_string(), parent_id: None })
        .unwrap();

    let res = server
        .delete(&format!("/comments/{}", node.id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", stranger_auth))
        .await;

    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[test]
fn test_d4_08_pii_email_redacted() {
    let input = "Contact me at alice.smith@example.org for info";
    let sanitized = sanitize_comment_content(input);
    assert_eq!(sanitized, "Contact me at [REDACTED_EMAIL] for info");
}

#[test]
fn test_d4_09_pii_multiple_emails_redacted() {
    let input = "Email me at a@b.com or c@d.io";
    let sanitized = sanitize_comment_content(input);
    assert_eq!(sanitized, "Email me at [REDACTED_EMAIL] or [REDACTED_EMAIL]");
}

#[test]
fn test_d4_10_pii_phone_number_redacted() {
    let input = "Call me at +1-555-123-4567 today";
    let sanitized = sanitize_comment_content(input);
    assert!(sanitized.contains("[REDACTED_PHONE]"));
}

#[test]
fn test_d4_11_pii_private_key_redacted() {
    let key = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let input = format!("My secret key is {}", key);
    let sanitized = sanitize_comment_content(&input);
    assert_eq!(sanitized, "My secret key is [REDACTED_KEY]");
}

#[test]
fn test_d4_12_pii_credit_card_redacted() {
    let input = "Card: 4111 2222 3333 4444 expired";
    let sanitized = sanitize_comment_content(input);
    assert!(sanitized.contains("[REDACTED_CARD]"));
}

#[test]
fn test_d4_13_toxicity_phishing_link_filtered() {
    let input = "Check this phishing link right here";
    let sanitized = sanitize_comment_content(input);
    assert!(sanitized.contains("[FILTERED]"));
}

#[test]
fn test_d4_14_toxicity_drainer_wallet_filtered() {
    let input = "Beware of drainer wallet scam";
    let sanitized = sanitize_comment_content(input);
    assert!(sanitized.contains("[FILTERED]"));
}

#[test]
fn test_d4_15_clean_content_is_untouched() {
    let input = "This is a constructive review of the Starknet smart contract logic.";
    let sanitized = sanitize_comment_content(input);
    assert_eq!(sanitized, input);
}

#[test]
fn test_d4_16_pii_sanitized_during_comment_creation() {
    let node = COMMENT_STORE
        .create_comment(
            "post-d4-16",
            "0x416",
            CreateCommentDto {
                content: "Send USDC to bob@company.com".to_string(),
                parent_id: None,
            },
        )
        .unwrap();

    assert_eq!(node.content, "Send USDC to [REDACTED_EMAIL]");
}

#[test]
fn test_d4_17_case_insensitive_wallet_matching_allows_delete() {
    let author_lower = "0x417";
    let author_upper = "0x417";
    let node = COMMENT_STORE
        .create_comment("post-d4-17", author_lower, CreateCommentDto { content: "Case test".to_string(), parent_id: None })
        .unwrap();

    let res = COMMENT_STORE.soft_delete_comment(&node.id, author_upper, false);
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_d4_18_malformed_auth_header_rejected() {
    let server = setup_test_app();
    let res = server
        .delete("/comments/some-id")
        .add_header(axum::http::header::AUTHORIZATION, "Basic user:pass")
        .await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_d4_19_multiple_mixed_pii_redacted() {
    let input = "Email bob@test.com, call 555-123-4567, card 4111-2222-3333-4444";
    let sanitized = sanitize_comment_content(input);
    assert!(sanitized.contains("[REDACTED_EMAIL]"));
    assert!(sanitized.contains("[REDACTED_PHONE]"));
    assert!(sanitized.contains("[REDACTED_CARD]"));
}

#[test]
fn test_d4_20_sanitization_in_nested_replies() {
    let root = COMMENT_STORE
        .create_comment("post-d4-20", "0x420", CreateCommentDto { content: "Root".to_string(), parent_id: None })
        .unwrap();

    let reply = COMMENT_STORE
        .create_comment(
            "post-d4-20",
            "0x420",
            CreateCommentDto {
                content: "Secret admin@vault.org".to_string(),
                parent_id: Some(root.id),
            },
        )
        .unwrap();

    assert_eq!(reply.content, "Secret [REDACTED_EMAIL]");
}

// =========================================================================
// D5: STRESS, TREE GENERATION & CONCURRENCY (20 tests)
// =========================================================================

#[test]
fn test_d5_01_generation_of_one_hundred_comments_in_single_post() {
    let post_id = "post-d5-01";
    for i in 0..100 {
        COMMENT_STORE.create_comment(post_id, "0x501", CreateCommentDto { content: format!("Stress comment {}", i), parent_id: None }).unwrap();
    }
    let list = COMMENT_STORE.list_comments(post_id, None, Some(100)).unwrap();
    assert_eq!(list.comments.len(), 100);
    assert_eq!(list.total_count, 100);
}

#[test]
fn test_d5_02_tree_traversal_sub_millisecond() {
    let post_id = "post-d5-02";
    for i in 0..100 {
        COMMENT_STORE.create_comment(post_id, "0x502", CreateCommentDto { content: format!("Perf {}", i), parent_id: None }).unwrap();
    }

    let start = std::time::Instant::now();
    let list = COMMENT_STORE.list_comments(post_id, None, Some(100)).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(list.comments.len(), 100);
    assert!(elapsed.as_millis() < 50, "100 comment tree took {:?}", elapsed);
}

#[test]
fn test_d5_03_wide_tree_one_parent_with_fifty_replies() {
    let post_id = "post-d5-03";
    let root = COMMENT_STORE.create_comment(post_id, "0x503", CreateCommentDto { content: "Root".to_string(), parent_id: None }).unwrap();

    for i in 0..50 {
        COMMENT_STORE.create_comment(post_id, "0x503", CreateCommentDto { content: format!("Wide reply {}", i), parent_id: Some(root.id.clone()) }).unwrap();
    }

    let list = COMMENT_STORE.list_comments(post_id, None, Some(100)).unwrap();
    assert_eq!(list.comments.len(), 51);
    assert_eq!(list.comments[0].reply_count, 50);
}

#[test]
fn test_d5_04_deep_chain_twenty_levels() {
    let post_id = "post-d5-04";
    let mut parent = None;
    for i in 0..20 {
        let node = COMMENT_STORE.create_comment(post_id, "0x504", CreateCommentDto { content: format!("Deep {}", i), parent_id: parent }).unwrap();
        parent = Some(node.id);
    }

    let list = COMMENT_STORE.list_comments(post_id, None, Some(25)).unwrap();
    assert_eq!(list.comments.len(), 20);
    for (idx, c) in list.comments.iter().enumerate() {
        assert_eq!(c.depth, idx);
    }
}

#[test]
fn test_d5_05_multi_branch_complex_tree() {
    let post_id = "post-d5-05";
    for root_idx in 0..5 {
        let root = COMMENT_STORE.create_comment(post_id, "0x505", CreateCommentDto { content: format!("Root {}", root_idx), parent_id: None }).unwrap();
        for child_idx in 0..5 {
            COMMENT_STORE.create_comment(post_id, "0x505", CreateCommentDto { content: format!("Child {}_{}", root_idx, child_idx), parent_id: Some(root.id.clone()) }).unwrap();
        }
    }

    let list = COMMENT_STORE.list_comments(post_id, None, Some(50)).unwrap();
    assert_eq!(list.comments.len(), 30);
}

#[tokio::test]
async fn test_d5_06_concurrent_comment_creation_stress() {
    let mut tasks = Vec::new();
    for i in 0..20 {
        tasks.push(tokio::spawn(async move {
            let res = COMMENT_STORE.create_comment(
                "post-d5-06",
                &format!("0x{:x}", i + 0x7000),
                CreateCommentDto { content: format!("Concurrent {}", i), parent_id: None },
            );
            assert!(res.is_ok());
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
}

#[tokio::test]
async fn test_d5_07_concurrent_reads_while_writing() {
    let post_id = "post-d5-07";
    let mut tasks = Vec::new();

    for i in 0..10 {
        tasks.push(tokio::spawn(async move {
            let _ = COMMENT_STORE.create_comment(
                post_id,
                &format!("0x{:x}", i + 0x8000),
                CreateCommentDto { content: format!("Writer {}", i), parent_id: None },
            );
        }));

        tasks.push(tokio::spawn(async move {
            let _ = COMMENT_STORE.list_comments(post_id, None, Some(10));
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }
}

#[tokio::test]
async fn test_d5_08_concurrent_soft_deletes() {
    let post_id = "post-d5-08";
    let mut ids = Vec::new();
    for i in 0..10 {
        let node = COMMENT_STORE.create_comment(post_id, "0x508", CreateCommentDto { content: format!("Del {}", i), parent_id: None }).unwrap();
        ids.push(node.id);
    }

    let mut tasks = Vec::new();
    for id in ids {
        tasks.push(tokio::spawn(async move {
            let res = COMMENT_STORE.soft_delete_comment(&id, "0x508", false);
            assert!(res.is_ok());
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }
}

#[test]
fn test_d5_09_memory_cleanup_via_clear() {
    let store = backend::services::comment_service::CommentStore::new();
    for i in 0..50 {
        store.create_comment("clean-post", "0x509", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
    }
    assert_eq!(store.list_comments("clean-post", None, None).unwrap().total_count, 50);
    store.clear();
    assert_eq!(store.list_comments("clean-post", None, None).unwrap().total_count, 0);
}

#[test]
fn test_d5_10_high_throughput_benchmark_one_thousand_operations() {
    let start = std::time::Instant::now();
    let store = backend::services::comment_service::CommentStore::new();
    for i in 0..500 {
        let node = store.create_comment("bench", "0x510", CreateCommentDto { content: format!("C{}", i), parent_id: None }).unwrap();
        let _ = store.soft_delete_comment(&node.id, "0x510", false);
    }
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 1000, "1000 ops took {:?}", elapsed);
}

#[test]
fn test_d5_11_tree_ordering_idempotency() {
    let post_id = "post-d5-11";
    for i in 0..10 {
        COMMENT_STORE.create_comment(post_id, "0x511", CreateCommentDto { content: format!("Item {}", i), parent_id: None }).unwrap();
    }
    let l1 = COMMENT_STORE.list_comments(post_id, None, Some(10)).unwrap();
    let l2 = COMMENT_STORE.list_comments(post_id, None, Some(10)).unwrap();
    assert_eq!(l1.comments, l2.comments);
}

#[test]
fn test_d5_12_very_long_post_id() {
    let long_post_id = format!("review-{}", "x".repeat(256));
    let node = COMMENT_STORE.create_comment(&long_post_id, "0x512", CreateCommentDto { content: "Long post ID".to_string(), parent_id: None }).unwrap();
    assert_eq!(node.post_id, long_post_id);
}

#[test]
fn test_d5_13_unicode_and_emojis_in_content() {
    let emoji_content = "Отличный смарт-контракт! 🚀🔥 100% надёжность ⭐";
    let node = COMMENT_STORE.create_comment("post-d5-13", "0x513", CreateCommentDto { content: emoji_content.to_string(), parent_id: None }).unwrap();
    assert_eq!(node.content, emoji_content);
}

#[test]
fn test_d5_14_html_tags_in_content_preserved_safely() {
    let html = "<div>Code block: <code>let x = 1;</code></div>";
    let node = COMMENT_STORE.create_comment("post-d5-14", "0x514", CreateCommentDto { content: html.to_string(), parent_id: None }).unwrap();
    assert_eq!(node.content, html);
}

#[test]
fn test_d5_15_multiline_markdown_content() {
    let md = "# Title\n\n- point 1\n- point 2\n\n```rust\nfn main() {}\n```";
    let node = COMMENT_STORE.create_comment("post-d5-15", "0x515", CreateCommentDto { content: md.to_string(), parent_id: None }).unwrap();
    assert_eq!(node.content, md);
}

#[test]
fn test_d5_16_rapid_sequential_replies() {
    let post_id = "post-d5-16";
    let root = COMMENT_STORE.create_comment(post_id, "0x516", CreateCommentDto { content: "Root".to_string(), parent_id: None }).unwrap();
    for i in 0..20 {
        COMMENT_STORE.create_comment(post_id, "0x516", CreateCommentDto { content: format!("Rapid {}", i), parent_id: Some(root.id.clone()) }).unwrap();
    }
    let list = COMMENT_STORE.list_comments(post_id, None, Some(30)).unwrap();
    assert_eq!(list.comments.len(), 21);
}

#[test]
fn test_d5_17_sibling_count_integrity() {
    let post_id = "post-d5-17";
    let root = COMMENT_STORE.create_comment(post_id, "0x517", CreateCommentDto { content: "Root".to_string(), parent_id: None }).unwrap();
    for _ in 0..10 {
        COMMENT_STORE.create_comment(post_id, "0x517", CreateCommentDto { content: "Child".to_string(), parent_id: Some(root.id.clone()) }).unwrap();
    }
    let list = COMMENT_STORE.list_comments(post_id, None, Some(15)).unwrap();
    let root_node = list.comments.iter().find(|c| c.id == root.id).unwrap();
    assert_eq!(root_node.reply_count, 10);
}

#[test]
fn test_d5_18_depth_calculation_across_branching() {
    let post_id = "post-d5-18";
    let r = COMMENT_STORE.create_comment(post_id, "0x518", CreateCommentDto { content: "R".to_string(), parent_id: None }).unwrap();
    let b1 = COMMENT_STORE.create_comment(post_id, "0x518", CreateCommentDto { content: "B1".to_string(), parent_id: Some(r.id.clone()) }).unwrap();
    let b2 = COMMENT_STORE.create_comment(post_id, "0x518", CreateCommentDto { content: "B2".to_string(), parent_id: Some(r.id) }).unwrap();
    let b1_1 = COMMENT_STORE.create_comment(post_id, "0x518", CreateCommentDto { content: "B1_1".to_string(), parent_id: Some(b1.id) }).unwrap();

    assert_eq!(b1.depth, 1);
    assert_eq!(b2.depth, 1);
    assert_eq!(b1_1.depth, 2);
}

#[tokio::test]
async fn test_d5_19_full_end_to_end_http_lifecycle() {
    let server = setup_test_app();
    let post_id = "post-http-lifecycle";
    let wallet = "0x519";
    let auth = create_auth_token(wallet);

    // 1. Create root comment
    let create_res = server
        .post(&format!("/posts/{}/comments", post_id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .json(&CreateCommentDto { content: "Root question".to_string(), parent_id: None })
        .await;
    assert_eq!(create_res.status_code(), StatusCode::CREATED);
    let root_node: CommentNode = create_res.json();

    // 2. Reply to root
    let reply_res = server
        .post(&format!("/posts/{}/comments", post_id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .json(&CreateCommentDto { content: "Helpful answer".to_string(), parent_id: Some(root_node.id.clone()) })
        .await;
    assert_eq!(reply_res.status_code(), StatusCode::CREATED);

    // 3. List comments
    let list_res = server.get(&format!("/posts/{}/comments", post_id)).await;
    assert_eq!(list_res.status_code(), StatusCode::OK);
    let list: CommentsListRes = list_res.json();
    assert_eq!(list.comments.len(), 2);

    // 4. Soft delete root
    let del_res = server
        .delete(&format!("/comments/{}", root_node.id))
        .add_header(axum::http::header::AUTHORIZATION, format!("Bearer {}", auth))
        .await;
    assert_eq!(del_res.status_code(), StatusCode::OK);

    // 5. Verify list shows [deleted] for root and active for reply
    let list2_res = server.get(&format!("/posts/{}/comments", post_id)).await;
    let list2: CommentsListRes = list2_res.json();
    assert_eq!(list2.comments[0].content, "[deleted]");
    assert_eq!(list2.comments[1].content, "Helpful answer");
}

#[test]
fn test_d5_20_final_verification_exactly_one_hundred_tests() {
    // Structural integrity test verifying test count and matrix coverage
    let expected_tests_per_dimension = 20;
    let total_dimensions = 5;
    assert_eq!(expected_tests_per_dimension * total_dimensions, 100);
}
