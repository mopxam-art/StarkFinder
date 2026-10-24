pub mod libs {
    pub mod apispec;
    pub mod config;
    pub mod db;
    pub mod error;
    pub mod jwt;
    pub mod logging;
    pub mod pagination;
    pub mod wallet;
}

pub mod middlewares {
    pub mod auth;
    pub mod request_id;
}

pub mod handlers {
    pub mod notifications;
}

pub mod models {
    pub mod notification;
}

pub mod services {
    pub mod notification_service;
}

pub mod routes {
    pub mod generate;
    pub mod health;
    pub mod register;
    pub mod reviews;
    pub mod user;
}

use axum::{
    Router,
    http::{
        Method,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    routing::{get, patch, post},
};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::{DefaultMakeSpan, TraceLayer},
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::libs::db::AppState;

pub fn create_app(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION]);

    // Router
    let app = Router::new()
        .route("/register", post(routes::register::register))
        .route("/user", get(routes::user::me))
        .route("/generate", post(routes::generate::generate_contract))
        .route(
            "/generated_contracts",
            get(routes::generate::list_generated_contracts),
        )
        .route("/reviews", get(routes::reviews::list_reviews))
        .route("/health", get(routes::health::health))
        .route(
            "/companies/{slug}/follow",
            post(handlers::notifications::follow_company)
                .delete(handlers::notifications::unfollow_company)
                .get(handlers::notifications::get_company_follow_status),
        )
        .route(
            "/tags/{name}/follow",
            post(handlers::notifications::follow_tag)
                .delete(handlers::notifications::unfollow_tag)
                .get(handlers::notifications::get_tag_follow_status),
        )
        .route(
            "/me/notifications",
            get(handlers::notifications::get_my_notifications)
                .patch(handlers::notifications::mark_all_notifications_as_read),
        )
        .route(
            "/me/notifications/read-all",
            post(handlers::notifications::mark_all_notifications_as_read),
        )
        .route(
            "/me/notifications/{id}",
            patch(handlers::notifications::mark_notification_as_read),
        )
        .route(
            "/notifications/dispatch",
            post(handlers::notifications::dispatch_notifications),
        )
        // Swagger UI at /docs and OpenAPI JSON at /api-docs/openapi.json
        .merge(SwaggerUi::new("/docs").url(
            "/api-docs/openapi.json",
            crate::libs::apispec::ApiDoc::openapi(),
        ));

    // request-id layers before trace
    middlewares::request_id::add_request_id(app)
        // trace requests (include headers so x-request-id is visible)
        .layer(
            TraceLayer::new_for_http().make_span_with(DefaultMakeSpan::new().include_headers(true)),
        )
        .layer(cors)
        .with_state(state)
}
