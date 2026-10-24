use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::company::{
        CompanyListResponse, CompanyResponse, CreateCompanyRequest, UpdateCompanyRequest,
    },
    services::company_service::{COMPANY_SERVICE, CompanyError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Company(CompanyError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<CompanyError> for HandlerError {
    fn from(e: CompanyError) -> Self {
        HandlerError::Company(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Company(e) => e.into_response(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct CompanyListQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Create a new company profile with auto-generated unique slug
#[utoipa::path(
    post,
    path = "/companies",
    tag = "companies",
    request_body = CreateCompanyRequest,
    responses(
        (status = 201, description = "Company created successfully", body = CompanyResponse),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody),
        (status = 409, description = "Domain conflict", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn create_company(
    Json(req): Json<CreateCompanyRequest>,
) -> Result<(StatusCode, Json<CompanyResponse>), HandlerError> {
    let company = COMPANY_SERVICE.create_company(req)?;
    Ok((StatusCode::CREATED, Json(company)))
}

/// Fetch public company profile by unique slug
#[utoipa::path(
    get,
    path = "/companies/{slug}",
    tag = "companies",
    params(
        ("slug" = String, Path, description = "Unique company slug")
    ),
    responses(
        (status = 200, description = "Company profile fetched successfully", body = CompanyResponse),
        (status = 404, description = "Company not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_company(
    Path(slug): Path<String>,
) -> Result<Json<CompanyResponse>, HandlerError> {
    let company = COMPANY_SERVICE.get_company_by_slug(&slug)?;
    Ok(Json(company))
}

/// Update company profile (Admin-only)
#[utoipa::path(
    put,
    path = "/companies/{slug}",
    tag = "companies",
    security(("bearer_auth" = [])),
    params(
        ("slug" = String, Path, description = "Unique company slug")
    ),
    request_body = UpdateCompanyRequest,
    responses(
        (status = 200, description = "Company updated successfully", body = CompanyResponse),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden - requires administrator privileges", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Company not found", body = crate::libs::error::ErrorBody),
        (status = 409, description = "Domain conflict with another company", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn update_company(
    AuthUser { wallet }: AuthUser,
    Path(slug): Path<String>,
    Json(req): Json<UpdateCompanyRequest>,
) -> Result<Json<CompanyResponse>, HandlerError> {
    let updated = COMPANY_SERVICE.update_company(&wallet, &slug, req)?;
    Ok(Json(updated))
}

/// List companies with pagination
#[utoipa::path(
    get,
    path = "/companies",
    tag = "companies",
    params(
        ("limit" = Option<usize>, Query, description = "Max number of items to return (default 10, max 100)"),
        ("offset" = Option<usize>, Query, description = "Offset index (default 0)")
    ),
    responses(
        (status = 200, description = "List of companies", body = CompanyListResponse)
    )
)]
pub async fn list_companies(
    Query(q): Query<CompanyListQuery>,
) -> Result<Json<CompanyListResponse>, HandlerError> {
    let limit = q.limit.unwrap_or(10);
    let offset = q.offset.unwrap_or(0);
    let list = COMPANY_SERVICE.list_companies(limit, offset);
    Ok(Json(list))
}
