use sqlx::{FromRow, MySqlConnection};
// WAIT 절은 SQL 리터럴로 유지한다(SQLx 0.9의 SQL 안전성 검사).
const _: () = assert!(crate::config::LOCK_WAIT_SECONDS == 3);

#[derive(FromRow)]
pub struct Account {
    pub id: u64,
    pub account_number: String,
    pub balance: i64,
}
// 내부 데이터와 응답 데이터가 같은 계좌번호를 사용한다.
pub struct Found {
    pub id: u64,
    pub number: String,
    pub balance: i64,
}
pub async fn by_user(conn: &mut MySqlConnection, user: u64) -> Result<Option<Found>, sqlx::Error> {
    let a = sqlx::query_as::<_, Account>(
        "SELECT id,account_number,balance FROM accounts WHERE user_id=?",
    )
    .bind(user)
    .fetch_optional(conn)
    .await?;
    Ok(a.map(|a| Found {
        id: a.id,
        number: a.account_number,
        balance: a.balance,
    }))
}
pub async fn lock(conn: &mut MySqlConnection, id: u64) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT id FROM accounts WHERE id=? FOR UPDATE WAIT 3")
        .bind(id)
        .fetch_one(conn)
        .await?;
    Ok(())
}
pub async fn lock_user(conn: &mut MySqlConnection, user: u64) -> Result<Option<u64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM accounts WHERE user_id=? FOR UPDATE WAIT 3")
        .bind(user)
        .fetch_optional(conn)
        .await
}
pub async fn add(conn: &mut MySqlConnection, id: u64, delta: i64) -> Result<bool, sqlx::Error> {
    Ok(
        sqlx::query("UPDATE accounts SET balance=balance+? WHERE id=? AND balance+?>=0")
            .bind(delta)
            .bind(id)
            .bind(delta)
            .execute(conn)
            .await?
            .rows_affected()
            == 1,
    )
}
pub async fn balance(conn: &mut MySqlConnection, id: u64) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT balance FROM accounts WHERE id=?")
        .bind(id)
        .fetch_one(conn)
        .await
}
pub async fn insert_ledger(
    conn: &mut MySqlConnection,
    id: u64,
    delta: i64,
    kind: &str,
    txid: &str,
    balance: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO ledger_entries (transaction_id,account_id,entry_type,amount,balance_after) VALUES (?,?,?,?,?)")
        .bind(txid).bind(id).bind(kind).bind(delta).bind(balance).execute(conn).await?;
    Ok(())
}
#[derive(FromRow)]
pub struct Entry {
    pub transaction_id: String,
    pub entry_type: String,
    pub amount: i64,
    pub balance_after: i64,
    pub created_at: chrono::NaiveDateTime,
}
pub async fn ledger(
    conn: &mut MySqlConnection,
    id: u64,
    limit: i64,
    offset: i64,
) -> Result<Vec<Entry>, sqlx::Error> {
    sqlx::query_as("SELECT transaction_id,entry_type,amount,balance_after,created_at FROM ledger_entries WHERE account_id=? ORDER BY id DESC LIMIT ? OFFSET ?")
        .bind(id).bind(limit).bind(offset).fetch_all(conn).await
}
