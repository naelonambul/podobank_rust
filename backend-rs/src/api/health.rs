//! backend/app/api/health/router.py

use axum::Json;
use axum::extract::State;
use mongodb::bson::doc;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::MONGO_SOCKET_TIMEOUT;
use crate::error::AppError;

pub async fn health(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|_| AppError::new(503, "database_unavailable", "데이터베이스에 연결할 수 없습니다"))?;

    let ping = tokio::time::timeout(MONGO_SOCKET_TIMEOUT, state.mongo.run_command(doc! {"ping": 1})).await;
    let mongodb = if matches!(ping, Ok(Ok(_))) { "ok" } else { "down" };
    Ok(Json(json!({"mariadb": "ok", "mongodb": mongodb})))
}
