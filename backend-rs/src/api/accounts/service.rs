use super::super::transactions::service::Failure;
use super::repository;
use crate::error::AppError;
use sqlx::MySqlConnection;

pub fn not_found() -> AppError {
    AppError::new(404, "account_not_found", "계좌를 찾을 수 없습니다")
}

pub async fn move_money(
    conn: &mut MySqlConnection,
    id: u64,
    delta: i64,
    kind: &str,
    txid: &str,
) -> Result<i64, Failure> {
    if !repository::add(conn, id, delta).await? {
        return Err(AppError::new(409, "insufficient_funds", "잔액이 부족합니다").into());
    }
    let balance = repository::balance(conn, id).await?;
    repository::insert_ledger(conn, id, delta, kind, txid, balance).await?;
    Ok(balance)
}
