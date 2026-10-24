use axum::{
    Json,
    extract::Query,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::verify::{
        StartVerificationRequest, StartVerificationResponse, SubmitProofRequest,
        SubmitProofResponse, VerificationStatusResponse,
    },
    services::verify_service::{VERIFY_SERVICE, VerifyError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Verify(VerifyError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<VerifyError> for HandlerError {
    fn from(e: VerifyError) -> Self {
        HandlerError::Verify(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Verify(e) => e.into_response(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct VerifyStatusQuery {
    pub company: String,
}

/// Initialize a new zkTLS employment verification session
#[utoipa::path(
    post,
    path = "/verify/employment/start",
    tag = "verify",
    security(("bearer_auth" = [])),
    request_body = StartVerificationRequest,
    responses(
        (status = 200, description = "Verification session initialized", body = StartVerificationResponse),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn start_verification(
    AuthUser { wallet }: AuthUser,
    Json(req): Json<StartVerificationRequest>,
) -> Result<Json<StartVerificationResponse>, HandlerError> {
    let resp = VERIFY_SERVICE.start_session(&wallet, req)?;
    Ok(Json(resp))
}

/// Submit generated zkTLS proof for validation and attestation
#[utoipa::path(
    post,
    path = "/verify/employment/submit",
    tag = "verify",
    security(("bearer_auth" = [])),
    request_body = SubmitProofRequest,
    responses(
        (status = 200, description = "Proof submission evaluated", body = SubmitProofResponse),
        (status = 400, description = "Bad request - invalid proof payload or expired session", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden - session ownership mismatch", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Session not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn submit_proof(
    AuthUser { wallet }: AuthUser,
    Json(req): Json<SubmitProofRequest>,
) -> Result<Json<SubmitProofResponse>, HandlerError> {
    let resp = VERIFY_SERVICE.submit_proof(&wallet, req)?;
    Ok(Json(resp))
}

/// Retrieve employment verification status for a company
#[utoipa::path(
    get,
    path = "/verify/employment/status",
    tag = "verify",
    security(("bearer_auth" = [])),
    params(
        ("company" = String, Query, description = "Company slug")
    ),
    responses(
        (status = 200, description = "Current verification status", body = VerificationStatusResponse),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_verification_status(
    AuthUser { wallet }: AuthUser,
    Query(q): Query<VerifyStatusQuery>,
) -> Result<Json<VerificationStatusResponse>, HandlerError> {
    let resp = VERIFY_SERVICE.get_status(&wallet, &q.company)?;
    Ok(Json(resp))
}
