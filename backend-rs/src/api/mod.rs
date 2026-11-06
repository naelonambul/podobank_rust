use axum::Router;
use axum::routing::get;

use crate::AppState;

pub mod health;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health::health))
}
