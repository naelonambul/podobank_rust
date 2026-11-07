pub mod service;
use super::{auth::service::Customer, events::Context};
use crate::{
    AppState,
    error::AppError,
    input::{self, Body},
};
use axum::{Json, Router, extract::State, routing::post};
use serde_json::Value;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/transactions/deposit", post(deposit))
        .route("/transactions/withdrawal", post(withdrawal))
}
async fn deposit(
    State(state): State<AppState>,
    Customer(user): Customer,
    ctx: Context,
    Body(body): Body,
) -> Result<Json<Value>, AppError> {
    let amount = input::amount(&body)?;
    service::run(
        &state,
        &ctx,
        user.id,
        service::Action::Cash {
            amount,
            kind: "deposit",
        },
    )
    .await
    .map(Json)
}
async fn withdrawal(
    State(state): State<AppState>,
    Customer(user): Customer,
    ctx: Context,
    Body(body): Body,
) -> Result<Json<Value>, AppError> {
    let amount = input::amount(&body)?;
    service::run(
        &state,
        &ctx,
        user.id,
        service::Action::Cash {
            amount,
            kind: "withdrawal",
        },
    )
    .await
    .map(Json)
}
