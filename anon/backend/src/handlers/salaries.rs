use axum::{
    Json,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode, header},
};
use std::str::FromStr;

use crate::{
    libs::{error::ApiError, jwt, wallet},
    models::salary::{
        CurrencyCode, SalaryAggregate, SalaryReport, SalarySummaryQuery, SubmitSalaryReq,
    },
    services::salary_service::SALARY_STORE,
};

fn extract_wallet_optional(headers: &HeaderMap) -> Option<String> {
    let hdr = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = hdr
        .strip_prefix("Bearer ")
        .or_else(|| hdr.strip_prefix("bearer "))?;
    let secret = jwt::secret_from_env();
    let claims = jwt::decode(token, &secret).ok()?;
    wallet::normalize_and_validate(&claims.sub).ok()
}

#[utoipa::path(
    post,
    path = "/salaries",
    tag = "salaries",
    request_body = SubmitSalaryReq,
    responses(
        (status = 201, description = "Salary report submitted successfully", body = SalaryReport),
        (status = 400, description = "Validation error", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn submit_salary_report(
    headers: HeaderMap,
    Json(payload): Json<SubmitSalaryReq>,
) -> Result<(StatusCode, Json<SalaryReport>), ApiError> {
    let user_wallet = extract_wallet_optional(&headers);
    let report = SALARY_STORE.submit_report(payload, user_wallet)?;
    Ok((StatusCode::CREATED, Json(report)))
}

#[utoipa::path(
    get,
    path = "/companies/{id}/salaries/summary",
    tag = "salaries",
    params(
        ("id" = String, Path, description = "Company identifier"),
        ("job_title" = Option<String>, Query, description = "Filter by job title"),
        ("currency" = Option<String>, Query, description = "Target currency (USD, EUR, GBP, USDC, ETH, STRK)")
    ),
    responses(
        (status = 200, description = "Aggregated salary statistics and percentiles", body = SalaryAggregate),
        (status = 400, description = "Invalid company identifier or unsupported currency", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_company_salary_summary(
    Path(id): Path<String>,
    Query(query): Query<SalarySummaryQuery>,
) -> Result<Json<SalaryAggregate>, ApiError> {
    let target_currency = match query.currency {
        Some(ref c) if !c.trim().is_empty() => Some(CurrencyCode::from_str(c).map_err(|_| {
            ApiError::BadRequest("unsupported currency: must be USD, EUR, GBP, USDC, ETH, or STRK")
        })?),
        _ => None,
    };

    let summary = SALARY_STORE.get_company_summary(
        &id,
        query.job_title.as_deref(),
        target_currency,
    )?;

    Ok(Json(summary))
}
