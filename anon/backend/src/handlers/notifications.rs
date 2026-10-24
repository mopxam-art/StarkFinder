use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    middlewares::auth::AuthUser,
    models::notification::{
        DispatchNotificationDto, DispatchResponse, FollowKind, FollowResponse, FollowStatusResponse,
        MarkAllReadResponse, MarkReadResponse, NotificationsListRes,
        NotificationsQuery, UnfollowResponse,
    },
    services::notification_service::{NOTIFICATION_STORE, NotificationError},
};

// ============================================================================
// 1. COMPANY FOLLOW HANDLERS
// ============================================================================

/// Subscribe current authenticated user to company alerts
#[utoipa::path(
    post,
    path = "/companies/{slug}/follow",
    tag = "notifications",
    params(
        ("slug" = String, Path, description = "Company unique slug")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Successfully subscribed to company", body = FollowResponse),
        (status = 400, description = "Invalid slug format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn follow_company(
    AuthUser { wallet }: AuthUser,
    Path(slug): Path<String>,
) -> Result<(StatusCode, Json<FollowResponse>), NotificationError> {
    let follow = NOTIFICATION_STORE.follow_target(&wallet, FollowKind::Company, &slug)?;

    Ok((
        StatusCode::OK,
        Json(FollowResponse {
            follower_user_id: follow.follower_user_id,
            kind: follow.kind,
            target_id: follow.target_id,
            created_at: follow.created_at,
            message: format!("Successfully followed company '{}'", slug),
        }),
    ))
}

/// Unsubscribe current user from company alerts
#[utoipa::path(
    delete,
    path = "/companies/{slug}/follow",
    tag = "notifications",
    params(
        ("slug" = String, Path, description = "Company unique slug")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Successfully unfollowed company", body = UnfollowResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Not currently following company", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn unfollow_company(
    AuthUser { wallet }: AuthUser,
    Path(slug): Path<String>,
) -> Result<Json<UnfollowResponse>, NotificationError> {
    NOTIFICATION_STORE.unfollow_target(&wallet, FollowKind::Company, &slug)?;

    Ok(Json(UnfollowResponse {
        message: format!("Successfully unfollowed company '{}'", slug),
        kind: FollowKind::Company,
        target_id: slug,
    }))
}

/// Check if current user follows company
#[utoipa::path(
    get,
    path = "/companies/{slug}/follow",
    tag = "notifications",
    params(
        ("slug" = String, Path, description = "Company unique slug")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Follow status", body = FollowStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_company_follow_status(
    AuthUser { wallet }: AuthUser,
    Path(slug): Path<String>,
) -> Json<FollowStatusResponse> {
    let is_following = NOTIFICATION_STORE.is_following(&wallet, FollowKind::Company, &slug);

    Json(FollowStatusResponse {
        is_following,
        kind: FollowKind::Company,
        target_id: slug,
    })
}

// ============================================================================
// 2. TAG FOLLOW HANDLERS
// ============================================================================

/// Subscribe current authenticated user to tag alerts
#[utoipa::path(
    post,
    path = "/tags/{name}/follow",
    tag = "notifications",
    params(
        ("name" = String, Path, description = "Tag identifier")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Successfully subscribed to tag", body = FollowResponse),
        (status = 400, description = "Invalid tag format", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn follow_tag(
    AuthUser { wallet }: AuthUser,
    Path(name): Path<String>,
) -> Result<(StatusCode, Json<FollowResponse>), NotificationError> {
    let follow = NOTIFICATION_STORE.follow_target(&wallet, FollowKind::Tag, &name)?;

    Ok((
        StatusCode::OK,
        Json(FollowResponse {
            follower_user_id: follow.follower_user_id,
            kind: follow.kind,
            target_id: follow.target_id,
            created_at: follow.created_at,
            message: format!("Successfully followed tag '{}'", name),
        }),
    ))
}

/// Unsubscribe current user from tag alerts
#[utoipa::path(
    delete,
    path = "/tags/{name}/follow",
    tag = "notifications",
    params(
        ("name" = String, Path, description = "Tag identifier")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Successfully unfollowed tag", body = UnfollowResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Not currently following tag", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn unfollow_tag(
    AuthUser { wallet }: AuthUser,
    Path(name): Path<String>,
) -> Result<Json<UnfollowResponse>, NotificationError> {
    NOTIFICATION_STORE.unfollow_target(&wallet, FollowKind::Tag, &name)?;

    Ok(Json(UnfollowResponse {
        message: format!("Successfully unfollowed tag '{}'", name),
        kind: FollowKind::Tag,
        target_id: name,
    }))
}

/// Check if current user follows tag
#[utoipa::path(
    get,
    path = "/tags/{name}/follow",
    tag = "notifications",
    params(
        ("name" = String, Path, description = "Tag identifier")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Follow status", body = FollowStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_tag_follow_status(
    AuthUser { wallet }: AuthUser,
    Path(name): Path<String>,
) -> Json<FollowStatusResponse> {
    let is_following = NOTIFICATION_STORE.is_following(&wallet, FollowKind::Tag, &name);

    Json(FollowStatusResponse {
        is_following,
        kind: FollowKind::Tag,
        target_id: name,
    })
}

// ============================================================================
// 3. IN-APP USER NOTIFICATIONS
// ============================================================================

/// List current user's in-app alerts with unread filtering and cursor pagination
#[utoipa::path(
    get,
    path = "/me/notifications",
    tag = "notifications",
    params(
        ("cursor" = Option<String>, Query, description = "Cursor notification ID for pagination"),
        ("limit" = Option<usize>, Query, description = "Max number of items to return"),
        ("only_unread" = Option<bool>, Query, description = "Filter strictly for unread notifications")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User notifications list", body = NotificationsListRes),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_my_notifications(
    AuthUser { wallet }: AuthUser,
    Query(query): Query<NotificationsQuery>,
) -> Json<NotificationsListRes> {
    let res = NOTIFICATION_STORE.get_user_notifications(
        &wallet,
        query.cursor.as_deref(),
        query.limit,
        query.only_unread.unwrap_or(false),
    );

    Json(res)
}

/// Mark a single notification as read by ID
#[utoipa::path(
    patch,
    path = "/me/notifications/{id}",
    tag = "notifications",
    params(
        ("id" = Uuid, Path, description = "Notification UUID")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Notification marked as read", body = MarkReadResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Access denied to notification", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Notification not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn mark_notification_as_read(
    AuthUser { wallet }: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<MarkReadResponse>, NotificationError> {
    let notification = NOTIFICATION_STORE.mark_as_read(&wallet, id)?;

    Ok(Json(MarkReadResponse {
        notification,
        message: "Notification marked as read".to_string(),
    }))
}

/// Mark all unread notifications for current user as read
#[utoipa::path(
    post,
    path = "/me/notifications/read-all",
    tag = "notifications",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "All unread notifications marked as read", body = MarkAllReadResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn mark_all_notifications_as_read(
    AuthUser { wallet }: AuthUser,
) -> Result<Json<MarkAllReadResponse>, NotificationError> {
    let count = NOTIFICATION_STORE.mark_all_as_read(&wallet)?;

    Ok(Json(MarkAllReadResponse {
        updated_count: count,
        message: format!("Marked {} notifications as read", count),
    }))
}

// ============================================================================
// 4. DISPATCH IN-APP NOTIFICATIONS (SYSTEM / INTERNAL)
// ============================================================================

/// Dispatch notification alerts to subscribers
#[utoipa::path(
    post,
    path = "/notifications/dispatch",
    tag = "notifications",
    request_body = DispatchNotificationDto,
    responses(
        (status = 200, description = "Notifications dispatched", body = DispatchResponse),
        (status = 400, description = "Invalid payload or targets", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn dispatch_notifications(
    Json(dto): Json<DispatchNotificationDto>,
) -> Result<Json<DispatchResponse>, NotificationError> {
    let count = NOTIFICATION_STORE.dispatch_notification(
        &dto.target_users,
        dto.notification_type,
        dto.payload,
    )?;

    Ok(Json(DispatchResponse {
        dispatched_count: count,
        message: format!("Successfully dispatched {} notifications", count),
    }))
}
