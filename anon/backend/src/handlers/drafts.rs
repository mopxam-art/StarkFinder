use axum::{
    Json,
    extract::Path,
    http::StatusCode,
};
use uuid::Uuid;

use crate::{
    middlewares::auth::AuthUser,
    models::draft::{
        AuditHistoryResponse, CreateDraftDto, DeleteDraftResponse, DraftResponse,
        EditPublishedReviewDto, EditPublishedReviewResponse, PublishResponse, PublishedReview,
        ReviewDraft, UpdateDraftDto,
    },
    services::draft_service::{DRAFT_SERVICE, DraftError},
};

/// Create a new review draft for the authenticated author
#[utoipa::path(
    post,
    path = "/posts/drafts",
    tag = "drafts",
    security(("bearer_auth" = [])),
    request_body = CreateDraftDto,
    responses(
        (status = 201, description = "Draft created successfully", body = DraftResponse),
        (status = 400, description = "Invalid input", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn create_draft(
    AuthUser { wallet }: AuthUser,
    Json(dto): Json<CreateDraftDto>,
) -> Result<(StatusCode, Json<DraftResponse>), DraftError> {
    let draft = DRAFT_SERVICE.create_draft(&wallet, dto)?;

    Ok((
        StatusCode::CREATED,
        Json(DraftResponse {
            draft,
            message: "Draft created successfully".to_string(),
        }),
    ))
}

/// Update an existing draft owned by the authenticated author
#[utoipa::path(
    put,
    path = "/posts/drafts/{id}",
    tag = "drafts",
    params(
        ("id" = Uuid, Path, description = "Draft unique ID")
    ),
    security(("bearer_auth" = [])),
    request_body = UpdateDraftDto,
    responses(
        (status = 200, description = "Draft updated successfully", body = DraftResponse),
        (status = 400, description = "Invalid input", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden: not the draft author", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Draft not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn update_draft(
    AuthUser { wallet }: AuthUser,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateDraftDto>,
) -> Result<Json<DraftResponse>, DraftError> {
    let draft = DRAFT_SERVICE.update_draft(&wallet, id, dto)?;

    Ok(Json(DraftResponse {
        draft,
        message: "Draft updated successfully".to_string(),
    }))
}

/// Delete a review draft
#[utoipa::path(
    delete,
    path = "/posts/drafts/{id}",
    tag = "drafts",
    params(
        ("id" = Uuid, Path, description = "Draft unique ID")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Draft deleted successfully", body = DeleteDraftResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden: not the draft author", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Draft not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn delete_draft(
    AuthUser { wallet }: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteDraftResponse>, DraftError> {
    DRAFT_SERVICE.delete_draft(&wallet, id)?;

    Ok(Json(DeleteDraftResponse {
        id,
        message: "Draft deleted successfully".to_string(),
    }))
}

/// Publish a review draft into a live review
#[utoipa::path(
    post,
    path = "/posts/drafts/{id}/publish",
    tag = "drafts",
    params(
        ("id" = Uuid, Path, description = "Draft unique ID")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Draft published successfully", body = PublishResponse),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden: not the draft author", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Draft not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn publish_draft(
    AuthUser { wallet }: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<PublishResponse>, DraftError> {
    let published = DRAFT_SERVICE.publish_draft(&wallet, id)?;

    Ok(Json(PublishResponse {
        published_review: published,
        message: "Review published successfully".to_string(),
    }))
}

/// Edit a published review within the allowed time window (records audit log)
#[utoipa::path(
    patch,
    path = "/posts/{id}",
    tag = "drafts",
    params(
        ("id" = Uuid, Path, description = "Published Review ID")
    ),
    security(("bearer_auth" = [])),
    request_body = EditPublishedReviewDto,
    responses(
        (status = 200, description = "Review edited successfully with audit log", body = EditPublishedReviewResponse),
        (status = 400, description = "Edit window expired or invalid content", body = crate::libs::error::ErrorBody),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden: not the review author", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Review not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn edit_published_review(
    AuthUser { wallet }: AuthUser,
    Path(id): Path<Uuid>,
    Json(dto): Json<EditPublishedReviewDto>,
) -> Result<Json<EditPublishedReviewResponse>, DraftError> {
    let (published_review, audit_entry) =
        DRAFT_SERVICE.edit_published_review(&wallet, id, &dto.content, dto.edit_reason)?;

    Ok(Json(EditPublishedReviewResponse {
        published_review,
        audit_entry,
        message: "Review edited successfully".to_string(),
    }))
}

/// Retrieve single draft by ID
#[utoipa::path(
    get,
    path = "/posts/drafts/{id}",
    tag = "drafts",
    params(
        ("id" = Uuid, Path, description = "Draft unique ID")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Draft details", body = ReviewDraft),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody),
        (status = 403, description = "Forbidden: not the draft author", body = crate::libs::error::ErrorBody),
        (status = 404, description = "Draft not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_draft(
    AuthUser { wallet }: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ReviewDraft>, DraftError> {
    let draft = DRAFT_SERVICE.get_draft(&wallet, id)?;
    Ok(Json(draft))
}

/// List all drafts for the authenticated author
#[utoipa::path(
    get,
    path = "/posts/drafts",
    tag = "drafts",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user drafts", body = [ReviewDraft]),
        (status = 401, description = "Unauthorized", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn list_my_drafts(
    AuthUser { wallet }: AuthUser,
) -> Json<Vec<ReviewDraft>> {
    let drafts = DRAFT_SERVICE.list_user_drafts(&wallet);
    Json(drafts)
}

/// Retrieve published review by ID
#[utoipa::path(
    get,
    path = "/posts/{id}",
    tag = "drafts",
    params(
        ("id" = Uuid, Path, description = "Published Review ID")
    ),
    responses(
        (status = 200, description = "Published review details", body = PublishedReview),
        (status = 404, description = "Review not found", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn get_published_review(
    Path(id): Path<Uuid>,
) -> Result<Json<PublishedReview>, DraftError> {
    let rev = DRAFT_SERVICE.get_published_review(id)?;
    Ok(Json(rev))
}

/// Retrieve audit history for a review
#[utoipa::path(
    get,
    path = "/posts/{id}/audit",
    tag = "drafts",
    params(
        ("id" = Uuid, Path, description = "Review ID")
    ),
    responses(
        (status = 200, description = "Audit log history", body = AuditHistoryResponse)
    )
)]
pub async fn get_review_audit_history(
    Path(id): Path<Uuid>,
) -> Json<AuditHistoryResponse> {
    let entries = DRAFT_SERVICE.get_review_audit_history(id);
    Json(AuditHistoryResponse {
        review_id: id,
        entries,
    })
}
