use super::{
    super::{accounts, events, transactions::service::Failure},
    repository,
};
use crate::error::AppError;
use serde_json::{Value, json};
use sqlx::MySqlConnection;

pub fn not_found() -> AppError {
    AppError::new(404, "recipient_not_found", "받는 사람을 찾을 수 없습니다")
}
pub async fn find(
    conn: &mut MySqlConnection,
    query: &str,
) -> Result<Option<repository::Recipient>, sqlx::Error> {
    let query = query.trim();
    let digits = query.replace('-', "");
    if digits.starts_with("01")
        && (10..=11).contains(&digits.len())
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        return repository::by_phone(conn, &digits).await;
    }
    let bytes = query.as_bytes();
    if bytes.len() == 13
        && bytes[3] == b'-'
        && bytes[8] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 3 || i == 8 || b.is_ascii_digit())
    {
        return repository::by_number(conn, query).await;
    }
    Ok(None)
}
pub async fn transfer(
    conn: &mut MySqlConnection,
    user: u64,
    query: &str,
    amount: i64,
) -> Result<Value, Failure> {
    let sender = accounts::repository::by_user(conn, user)
        .await?
        .ok_or_else(accounts::service::not_found)?;
    let receiver = find(conn, query).await?.ok_or_else(not_found)?;
    if sender.id == receiver.id {
        return Err(AppError::new(422, "self_transfer", "내 계좌로는 보낼 수 없습니다").into());
    }
    for id in [sender.id.min(receiver.id), sender.id.max(receiver.id)] {
        accounts::repository::lock(conn, id).await?;
    }
    let start = events::now().date().and_hms_opt(0, 0, 0).unwrap();
    let end = start + chrono::Duration::days(1);
    if repository::sent(conn, sender.id, start, end).await? + amount > 100_000_000 {
        return Err(AppError::new(
            422,
            "daily_limit_exceeded",
            "하루 송금 한도 100,000,000원을 넘습니다",
        )
        .into());
    }
    let txid = uuid::Uuid::new_v4().to_string();
    let balance =
        accounts::service::move_money(conn, sender.id, -amount, "transfer_out", &txid).await?;
    accounts::service::move_money(conn, receiver.id, amount, "transfer_in", &txid).await?;
    Ok(json!({"transaction_id":txid,"balance":balance}))
}
