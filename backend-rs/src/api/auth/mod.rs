pub mod repository;
pub mod service;

use super::events::{self, Context};
use crate::{
    AppState,
    error::AppError,
    input::{self, Body},
};
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use service::User;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/signup", post(signup))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}

fn logged_in(state: &AppState, token: &str, device: &str) -> Response {
    let mut response = Json(json!({"status":"authenticated"})).into_response();
    let secure = if state.settings.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    for (name, value, age) in [
        ("podobank_session", token, 3600),
        ("podobank_device", device, 31_536_000),
    ] {
        response.headers_mut().append(
            "set-cookie",
            format!("{name}={value}; HttpOnly; Max-Age={age}; Path=/; SameSite=lax{secure}")
                .parse()
                .unwrap(),
        );
    }
    response
}

async fn login(
    State(state): State<AppState>,
    ctx: Context,
    headers: HeaderMap,
    Body(body): Body,
) -> Result<Response, AppError> {
    let mut validation = input::Validation::default();
    let phone = validation.take(input::string(&body, "phone_number", 1, 20));
    let password = validation.take(input::string(&body, "password", 1, 128));
    validation.finish()?;
    let (token, device) = service::login(&state, &ctx, &headers, phone, password).await?;
    Ok(logged_in(&state, &token, &device))
}
async fn signup(
    State(state): State<AppState>,
    ctx: Context,
    headers: HeaderMap,
    Body(body): Body,
) -> Result<Response, AppError> {
    let data = service::Signup::parse(&body)?;
    let user_id = service::signup(&state, &ctx, data).await?;
    let device = service::device(&headers);
    let mut tx = state.db.begin().await?;
    let token = repository::start_session(&mut tx, user_id, &device, &headers).await?;
    tx.commit().await?;
    Ok(logged_in(&state, &token, &device))
}
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    if let Some(token) = service::cookie(&headers, "podobank_session") {
        sqlx::query("UPDATE sessions SET ended_at=?, end_reason='logout' WHERE token_hash=? AND ended_at IS NULL")
            .bind(events::now()).bind(crate::security::token_hash(&token)).execute(&state.db).await?;
    }
    let mut response = Json(json!({"status":"logged_out"})).into_response();
    let secure = if state.settings.cookie_secure {
        "; Secure"
    } else {
        ""
    };
    response.headers_mut().insert(
        "set-cookie",
        format!("podobank_session=; HttpOnly; Max-Age=0; Path=/; SameSite=lax{secure}")
            .parse()
            .unwrap(),
    );
    Ok(response)
}
async fn me(user: User) -> Json<Value> {
    Json(json!({"id":user.id,"name":user.name,"role":user.role}))
}
