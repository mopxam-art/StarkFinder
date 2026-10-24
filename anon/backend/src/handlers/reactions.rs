use axum::{
    Json,
    extract::Path,
    http::{HeaderMap, header},
};
use std::str::FromStr;

use crate::{
    libs::{error::ApiError, jwt, wallet},
    middlewares::auth::AuthUser,
    models::reaction::{
        CreateReactionDto, ReactionResponse, ReactionSummary, ReactionTarget, ReactionType,
    },
    services::reaction_service::REACTION_STORE,
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
    path = "/reactions/{target_type}/{id}",
    tag = "reactions",
    security(("bearer_auth" = [])),
    params(
        ("target_type" = String, Path, description = "Target type: review or comment"),
        ("id" = String, Path, description = "Target identifier")
    ),
    request_body = CreateReactionDto,
    responses(
        (status = 200, description = "Reaction recorded or updated", body = ReactionResponse),
        (status = 400, description = "Invalid reaction type or target type", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Cannot react to own content", body = crate::libs::error::ErrorBody),
        (status = 429, description = "Rate limit exceeded", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn add_or_update_reaction(
    Path((target_type, id)): Path<(String, String)>,
    AuthUser { wallet }: AuthUser,
    Json(payload): Json<CreateReactionDto>,
) -> Result<Json<ReactionResponse>, ApiError> {
    let target = ReactionTarget::from_str(&target_type)
        .map_err(|_| ApiError::BadRequest("invalid target type: must be review or comment"))?;
    let reaction = ReactionType::from_str(&payload.reaction_type).map_err(|_| {
        ApiError::BadRequest("invalid reaction type: must be upvote, insightful, or thanks")
    })?;

    let res = REACTION_STORE.record_reaction(&wallet, target, &id, reaction)?;
    Ok(Json(res))
}

#[utoipa::path(
    delete,
    path = "/reactions/{target_type}/{id}",
    tag = "reactions",
    security(("bearer_auth" = [])),
    params(
        ("target_type" = String, Path, description = "Target type: review or comment"),
        ("id" = String, Path, description = "Target identifier")
    ),
    responses(
        (status = 200, description = "Reaction removed", body = ReactionResponse),
        (status = 400, description = "Invalid target type", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn remove_reaction(
    Path((target_type, id)): Path<(String, String)>,
    AuthUser { wallet }: AuthUser,
) -> Result<Json<ReactionResponse>, ApiError> {
    let target = ReactionTarget::from_str(&target_type)
        .map_err(|_| ApiError::BadRequest("invalid target type: must be review or comment"))?;

    let res = REACTION_STORE.remove_reaction(&wallet, target, &id)?;
    Ok(Json(res))
}

#[utoipa::path(
    get,
    path = "/reactions/{target_type}/{id}",
    tag = "reactions",
    params(
        ("target_type" = String, Path, description = "Target type: review or comment"),
        ("id" = String, Path, description = "Target identifier")
    ),
    responses(
        (status = 200, description = "Get reaction summary", body = ReactionSummary),
        (status = 400, description = "Invalid target type", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_reaction_summary(
    Path((target_type, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<ReactionSummary>, ApiError> {
    let target = ReactionTarget::from_str(&target_type)
        .map_err(|_| ApiError::BadRequest("invalid target type: must be review or comment"))?;

    let user_wallet = extract_wallet_optional(&headers);
    let summary = REACTION_STORE.get_summary(user_wallet.as_deref(), target, &id);
    Ok(Json(summary))
}
