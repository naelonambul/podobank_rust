pub mod repository;
pub mod service;
use super::{auth::service::Customer, events};
use crate::{AppState, error::AppError, input};
use axum::{
    Json, Router,
    extract::{RawQuery, State},
    routing::get,
};
use serde_json::{Value, json};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/accounts/me", get(account))
        .route("/accounts/me/ledger", get(ledger))
}
async fn account(
    State(state): State<AppState>,
    Customer(user): Customer,
) -> Result<Json<Value>, AppError> {
    let mut conn = state.db.acquire().await?;
    let account = repository::by_user(&mut conn, user.id)
        .await?
        .ok_or_else(service::not_found)?;
    Ok(Json(
        json!({"account_number":account.number,"balance":account.balance}),
    ))
}
async fn ledger(
    State(state): State<AppState>,
    Customer(user): Customer,
    RawQuery(query): RawQuery,
) -> Result<Json<Value>, AppError> {
    let mut limit = 20_i64;
    let mut offset = 0_i64;
    let mut validation = input::Validation::default();
    // 정상적인 숫자 쿼리 외에는 Python과 같은 필드 검증 오류를 반환한다.
    if let Some(query) = query {
        let parsed = reqwest::Url::parse(&format!("http://localhost/?{query}"))
            .map_err(|_| input::invalid("body"))?;
        for (key, value) in parsed.query_pairs() {
            if key == "limit" {
                limit = validation.take(input::integer(&json!({"limit":value}), "limit"));
            }
            if key == "offset" {
                offset = validation.take(input::integer(&json!({"offset":value}), "offset"));
            }
        }
    }
    validation.check("limit", (1..=100).contains(&limit));
    validation.check("offset", (0..=10_000).contains(&offset));
    validation.finish()?;
    let mut conn = state.db.acquire().await?;
    let account = repository::by_user(&mut conn, user.id)
        .await?
        .ok_or_else(service::not_found)?;
    let rows = repository::ledger(&mut conn, account.id, limit, offset).await?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({"transaction_id":r.transaction_id,"entry_type":r.entry_type,
        "amount":r.amount,"balance_after":r.balance_after,"created_at":events::iso(r.created_at)})
        })
        .collect();
    Ok(Json(json!({"items":items,"limit":limit,"offset":offset})))
}
