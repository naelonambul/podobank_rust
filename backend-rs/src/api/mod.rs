use axum::Router;
use axum::routing::get;

use crate::AppState;

pub mod accounts;
pub mod auth;
pub mod events;
pub mod health;
pub mod transactions;
pub mod transfers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .merge(auth::router())
        .merge(accounts::router())
        .merge(transactions::router())
        .merge(transfers::router())
        .method_not_allowed_fallback(|| async {
            crate::error::AppError::new(405, "method_not_allowed", "허용되지 않은 요청 방식입니다")
        })
}
