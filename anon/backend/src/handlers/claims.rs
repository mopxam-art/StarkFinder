use std::str::FromStr;

use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::claim::{
        AdminClaimDecisionRequest, CompanyClaim, CompanyClaimStatusResponse, SubmitClaimRequest,
    },
    services::claim_service::{CLAIM_SERVICE, ClaimError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Claim(ClaimError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<ClaimError> for HandlerError {
    fn from(e: ClaimError) -> Self {
        HandlerError::Claim(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Claim(e) => e.into_response(),
        }
    }
}

/// Submit a claim to become a verified company owner
#[utoipa::path(
    post,
    path = "/companies/{slug}/claim",
    tag = "claims",
    security(("bearer_auth" = [])),
    params(
        ("slug" = String, Path, description = "Company slug")
    ),
    request_body = SubmitClaimRequest,
    responses(
        (status = 201, description = "Company ownership claim submitted successfully", body = CompanyClaim),
        (status = 400, description = "Bad request (validation error)", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized (missing or invalid token)", body = crate::libs::error::ErrorBody),
        (status = 409, description = "Conflict (claim already pending or already owner)", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn submit_company_claim(
    AuthUser { wallet }: AuthUser,
    Path(slug): Path<String>,
    Json(req): Json<SubmitClaimRequest>,
) -> Result<(StatusCode, Json<CompanyClaim>), HandlerError> {
    let claim = CLAIM_SERVICE.submit_claim(&slug, &wallet, req)?;
    Ok((StatusCode::CREATED, Json(claim)))
}

/// Retrieve public claim and ownership status of a company
#[utoipa::path(
    get,
    path = "/companies/{slug}/claim-status",
    tag = "claims",
    params(
        ("slug" = String, Path, description = "Company slug")
    ),
    responses(
        (status = 200, description = "Public claim status of company", body = CompanyClaimStatusResponse)
    )
)]
pub async fn get_company_claim_status(
    Path(slug): Path<String>,
) -> Json<CompanyClaimStatusResponse> {
    let resp = CLAIM_SERVICE.get_company_claim_status(&slug);
    Json(resp)
}

/// Review and decide on a company ownership claim (Admin only)
#[utoipa::path(
    put,
    path = "/admin/companies/{slug}/claim/{id}",
    tag = "claims",
    security(("bearer_auth" = [])),
    params(
        ("slug" = String, Path, description = "Company slug"),
        ("id" = String, Path, description = "Claim UUID")
    ),
    request_body = AdminClaimDecisionRequest,
    responses(
        (status = 200, description = "Claim decision processed successfully", body = CompanyClaim),
        (status = 400, description = "Invalid UUID format or action", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Admin privileges required", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Claim not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn admin_decide_claim(
    AuthUser { wallet }: AuthUser,
    Path((slug, id_str)): Path<(String, String)>,
    Json(req): Json<AdminClaimDecisionRequest>,
) -> Result<Json<CompanyClaim>, HandlerError> {
    let claim_id = Uuid::from_str(&id_str)
        .map_err(|_| HandlerError::Api(ApiError::BadRequest("invalid claim id format")))?;

    let claim = CLAIM_SERVICE.admin_decide_claim(&slug, claim_id, &wallet, req)?;
    Ok(Json(claim))
}
