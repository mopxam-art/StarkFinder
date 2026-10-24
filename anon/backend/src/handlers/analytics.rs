use axum::{
    Json,
    extract::{Path, Query},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    libs::error::ApiError,
    middlewares::auth::AuthUser,
    models::analytics::{AnalyticsWindow, OwnerAnalyticsResponse},
    services::analytics_service::{ANALYTICS_SERVICE, AnalyticsError},
};

#[derive(Debug)]
pub enum HandlerError {
    Api(ApiError),
    Analytics(AnalyticsError),
}

impl From<ApiError> for HandlerError {
    fn from(e: ApiError) -> Self {
        HandlerError::Api(e)
    }
}

impl From<AnalyticsError> for HandlerError {
    fn from(e: AnalyticsError) -> Self {
        HandlerError::Analytics(e)
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        match self {
            HandlerError::Api(e) => e.into_response(),
            HandlerError::Analytics(e) => e.into_response(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct AnalyticsQuery {
    pub window: Option<String>,
}

/// Retrieve deep-dive privacy-preserving analytics for a claimed company (Owner-only)
#[utoipa::path(
    get,
    path = "/owner/companies/{slug}/analytics",
    tag = "analytics",
    security(("bearer_auth" = [])),
    params(
        ("slug" = String, Path, description = "Company slug"),
        ("window" = Option<String>, Query, description = "Time window: '30d' or '90d' (default: '30d')")
    ),
    responses(
        (status = 200, description = "Owner analytics response", body = OwnerAnalyticsResponse),
        (status = 400, description = "Bad request - invalid window parameter", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden - caller is not verified owner", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_owner_analytics(
    AuthUser { wallet }: AuthUser,
    Path(slug): Path<String>,
    Query(q): Query<AnalyticsQuery>,
) -> Result<Json<OwnerAnalyticsResponse>, HandlerError> {
    let window = match q.window.as_deref() {
        None | Some("30d") => AnalyticsWindow::ThirtyDays,
        Some("90d") => AnalyticsWindow::NinetyDays,
        Some(_) => {
            return Err(HandlerError::Api(ApiError::BadRequest(
                "invalid window parameter: must be '30d' or '90d'",
            )));
        }
    };

    let resp = ANALYTICS_SERVICE.get_owner_analytics(&wallet, &slug, window)?;
    Ok(Json(resp))
}
