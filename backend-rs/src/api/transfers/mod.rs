pub mod repository;
pub mod service;
use super::{auth::service::Customer, events::Context, transactions};
use crate::{
    AppState,
    error::AppError,
    input::{self, Body},
};
use axum::{Json, Router, extract::State, routing::post};
use serde_json::{Value, json};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/transfers/recipient-lookup", post(lookup))
        .route("/transfers", post(transfer))
}
async fn lookup(
    State(state): State<AppState>,
    Customer(_): Customer,
    Body(body): Body,
) -> Result<Json<Value>, AppError> {
    let query = input::string(&body, "query", 1, 20)?;
    let mut conn = state.db.acquire().await?;
    let receiver = service::find(&mut conn, &query)
        .await?
        .ok_or_else(service::not_found)?;
    Ok(Json(json!({"name":receiver.name})))
}
async fn transfer(
    State(state): State<AppState>,
    Customer(user): Customer,
    ctx: Context,
    Body(body): Body,
) -> Result<Json<Value>, AppError> {
    let mut validation = input::Validation::default();
    let recipient = validation.take(input::string(&body, "recipient", 1, 20));
    let amount = validation.take(input::amount(&body));
    validation.finish()?;
    transactions::service::run(
        &state,
        &ctx,
        user.id,
        transactions::service::Action::Transfer { amount, recipient },
    )
    .await
    .map(Json)
}
