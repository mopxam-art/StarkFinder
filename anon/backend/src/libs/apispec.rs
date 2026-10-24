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
        crate::handlers::drafts::create_draft,
        crate::handlers::drafts::update_draft,
        crate::handlers::drafts::delete_draft,
        crate::handlers::drafts::publish_draft,
        crate::handlers::drafts::edit_published_review,
        crate::handlers::drafts::get_draft,
        crate::handlers::drafts::list_my_drafts,
        crate::handlers::drafts::get_published_review,
        crate::handlers::drafts::get_review_audit_history
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
            // Drafts, Revisions & Audit
            crate::models::draft::ReviewDraft,
            crate::models::draft::ReviewAuditEntry,
            crate::models::draft::PublishedReview,
            crate::models::draft::CreateDraftDto,
            crate::models::draft::UpdateDraftDto,
            crate::models::draft::EditPublishedReviewDto,
            crate::models::draft::DraftResponse,
            crate::models::draft::DeleteDraftResponse,
            crate::models::draft::PublishResponse,
            crate::models::draft::EditPublishedReviewResponse,
            crate::models::draft::AuditHistoryResponse
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication & registration endpoints"),
        (name = "contracts", description = "Generated contracts endpoints"),
        (name = "reviews", description = "Reviews listing endpoints"),
        (name = "drafts", description = "Review drafts and time-windowed revision endpoints")
    )
)]
pub struct ApiDoc;
