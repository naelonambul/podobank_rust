use chrono::NaiveDateTime;
use sqlx::{FromRow, MySqlConnection};

#[derive(FromRow)]
pub struct Recipient {
    pub id: u64,
    pub name: String,
}
pub async fn by_phone(
    conn: &mut MySqlConnection,
    phone: &str,
) -> Result<Option<Recipient>, sqlx::Error> {
    sqlx::query_as("SELECT a.id,u.name FROM accounts a JOIN users u ON u.id=a.user_id WHERE u.phone_number=? AND u.role='customer'").bind(phone).fetch_optional(conn).await
}
pub async fn by_number(
    conn: &mut MySqlConnection,
    number: &str,
) -> Result<Option<Recipient>, sqlx::Error> {
    sqlx::query_as("SELECT a.id,u.name FROM accounts a JOIN users u ON u.id=a.user_id WHERE a.account_number=? AND u.role='customer'").bind(number).fetch_optional(conn).await
}
pub async fn sent(
    conn: &mut MySqlConnection,
    id: u64,
    start: NaiveDateTime,
    end: NaiveDateTime,
) -> Result<i64, sqlx::Error> {
    // SUM(BIGINT)는 DECIMAL이므로 한도 범위의 정수로 명시 변환한다.
    sqlx::query_scalar("SELECT CAST(COALESCE(-SUM(amount),0) AS SIGNED) FROM ledger_entries WHERE account_id=? AND entry_type='transfer_out' AND created_at>=? AND created_at<?")
        .bind(id).bind(start).bind(end).fetch_one(conn).await
}
