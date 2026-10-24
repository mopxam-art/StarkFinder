use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        // Take existing components or default, add security scheme, and put back.
        let mut components = openapi.components.take().unwrap_or_default();
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
        openapi.components = Some(components);
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::register::register,
        crate::routes::user::me,
        crate::routes::health::healthz,
        crate::routes::generate::generate_contract,
        crate::routes::generate::list_generated_contracts,
        crate::routes::reviews::list_reviews,
        crate::handlers::notifications::follow_company,
        crate::handlers::notifications::unfollow_company,
        crate::handlers::notifications::get_company_follow_status,
        crate::handlers::notifications::follow_tag,
        crate::handlers::notifications::unfollow_tag,
        crate::handlers::notifications::get_tag_follow_status,
        crate::handlers::notifications::get_my_notifications,
        crate::handlers::notifications::mark_notification_as_read,
        crate::handlers::notifications::mark_all_notifications_as_read,
        crate::handlers::notifications::dispatch_notifications
    ),
    components(
        schemas(
            crate::routes::register::RegisterReq,
            crate::routes::register::RegisterRes,
            crate::routes::user::UserMeRes,
            crate::routes::user::ProfilePublic,
            crate::libs::error::ErrorBody,
            crate::routes::health::HealthzResponse,
            // Contracts
            crate::routes::generate::GenerateContractReq,
            crate::routes::generate::GenerateContractRes,
            crate::routes::generate::GeneratedContractItem,
            crate::routes::generate::GeneratedContractsListRes,
            // Reviews
            crate::routes::reviews::ReviewItem,
            crate::routes::reviews::ReviewsListRes,
            // Notifications & Follows
            crate::models::notification::FollowKind,
            crate::models::notification::Follow,
            crate::models::notification::NotificationType,
            crate::models::notification::Notification,
            crate::models::notification::NotificationsQuery,
            crate::models::notification::NotificationsListRes,
            crate::models::notification::FollowResponse,
            crate::models::notification::UnfollowResponse,
            crate::models::notification::FollowStatusResponse,
            crate::models::notification::ListFollowersResponse,
            crate::models::notification::MarkReadResponse,
            crate::models::notification::MarkAllReadResponse,
            crate::models::notification::DispatchNotificationDto,
            crate::models::notification::DispatchResponse
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication & registration endpoints"),
        (name = "contracts", description = "Generated contracts endpoints"),
        (name = "reviews", description = "Reviews listing endpoints"),
        (name = "notifications", description = "Company/tag follows and in-app alerts")
    )
)]
pub struct ApiDoc;
