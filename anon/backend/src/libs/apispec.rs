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
        crate::handlers::company_responses::post_or_update_response,
        crate::handlers::company_responses::get_response,
        crate::handlers::company_responses::get_response_audit
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
            // Company Responses
            crate::models::company_response::CompanyResponse,
            crate::models::company_response::CompanyResponseAudit,
            crate::models::company_response::CreateOrUpdateResponseDto,
            crate::models::company_response::CompanyResponseRes,
            crate::models::company_response::CompanyResponseAuditRes
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication & registration endpoints"),
        (name = "contracts", description = "Generated contracts endpoints"),
        (name = "reviews", description = "Reviews listing endpoints"),
        (name = "company_responses", description = "Official company employer review response endpoints")
    )
)]
pub struct ApiDoc;
