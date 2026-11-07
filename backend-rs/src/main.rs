//! PodoBank API (Rust). backend/app/main.py의 create_app과 같은 구성.
//! 실행: podobank-api [--host 0.0.0.0] [--port 8000]

mod api;
mod config;
mod db;
mod error;
mod input;
mod request_id;
mod security;

use axum::Router;
use axum::middleware;
use sqlx::MySqlPool;

use crate::config::Settings;

const API_PREFIX: &str = "/api/v1";

#[derive(Clone)]
pub struct AppState {
    pub settings: Settings,
    pub db: MySqlPool,
    pub mongo: mongodb::Database,
}

fn arg(name: &str, default: &str) -> String {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| default.to_string())
}

pub fn create_app(state: AppState) -> Router {
    Router::new()
        .nest(API_PREFIX, api::router())
        .fallback(error::not_found)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            api::events::application_log,
        ))
        .layer(middleware::from_fn(request_id::request_id))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let settings = Settings::from_env().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let db = db::make_pool(&settings.mariadb_url).expect("MARIADB_URL");
    let mongo = db::make_mongo(&settings.mongodb_url, &settings.mongodb_db)
        .await
        .expect("MONGODB_URL");
    let state = AppState {
        settings,
        db,
        mongo,
    };
    api::events::ensure_indexes(&state).await;

    let addr = format!("{}:{}", arg("--host", "127.0.0.1"), arg("--port", "8000"));
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    axum::serve(listener, create_app(state))
        .await
        .expect("serve");
}
