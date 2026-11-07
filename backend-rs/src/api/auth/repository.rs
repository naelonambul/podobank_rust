use super::super::events;
use crate::{error::AppError, security};
use axum::http::HeaderMap;
use sqlx::{MySqlConnection, Row};

pub async fn start_session(
    conn: &mut MySqlConnection,
    user: u64,
    device: &str,
    headers: &HeaderMap,
) -> Result<String, AppError> {
    let now = events::now();
    let token = security::token()?;
    sqlx::query("UPDATE sessions SET ended_at=?, end_reason='replaced' WHERE user_id=? AND device_id=? AND ended_at IS NULL")
        .bind(now).bind(user).bind(device).execute(&mut *conn).await?;
    let channel = headers
        .get("x-client")
        .and_then(|v| v.to_str().ok())
        .filter(|v| matches!(*v, "web" | "ios-app" | "android-app"))
        .unwrap_or("web");
    let agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.chars().take(255).collect::<String>());
    sqlx::query("INSERT INTO sessions (token_hash,user_id,device_id,client,user_agent,login_method,created_at,last_seen_at) VALUES (?,?,?,?,?,'password',?,?)")
        .bind(security::token_hash(&token)).bind(user).bind(device).bind(channel).bind(agent).bind(now).bind(now).execute(&mut *conn).await?;
    Ok(token)
}

pub async fn resolve(
    state: &crate::AppState,
    token: &str,
) -> Result<super::service::User, AppError> {
    let mut tx = state.db.begin().await?;
    let row = sqlx::query("SELECT s.id,s.created_at,s.last_seen_at,u.id AS user_id,u.role,u.name FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND s.ended_at IS NULL")
        .bind(security::token_hash(token)).fetch_optional(&mut *tx).await?.ok_or_else(super::service::unauthorized)?;
    let now = events::now();
    let seen: chrono::NaiveDateTime = row.get("last_seen_at");
    let created: chrono::NaiveDateTime = row.get("created_at");
    let reason = if now - seen >= chrono::Duration::minutes(10) {
        Some("idle_timeout")
    } else if now - created >= chrono::Duration::minutes(60) {
        Some("max_age")
    } else {
        None
    };
    let id: u64 = row.get("id");
    if let Some(reason) = reason {
        sqlx::query("UPDATE sessions SET ended_at=?,end_reason=? WHERE id=? AND ended_at IS NULL")
            .bind(now)
            .bind(reason)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(super::service::unauthorized());
    }
    if now - seen >= chrono::Duration::seconds(60) {
        sqlx::query("UPDATE sessions SET last_seen_at=? WHERE id=? AND last_seen_at<=?")
            .bind(now)
            .bind(id)
            .bind(now - chrono::Duration::seconds(60))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(super::service::User {
        id: row.get("user_id"),
        role: row.get("role"),
        name: row.get("name"),
    })
}
