use super::super::{
    accounts,
    events::{self, Context},
    transfers,
};
use crate::{AppState, error::AppError, input};
use serde_json::{Value, json};
use sqlx::MySqlConnection;

pub enum Failure {
    Db(sqlx::Error),
    App(AppError),
}
impl From<sqlx::Error> for Failure {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}
impl From<AppError> for Failure {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}
pub enum Action {
    Cash { amount: i64, kind: &'static str },
    Transfer { amount: i64, recipient: String },
}
impl Action {
    fn kind(&self) -> &'static str {
        match self {
            Self::Cash { kind, .. } => kind,
            Self::Transfer { .. } => "transfer",
        }
    }
    fn amount(&self) -> i64 {
        match self {
            Self::Cash { amount, .. } | Self::Transfer { amount, .. } => *amount,
        }
    }
}
async fn execute(conn: &mut MySqlConnection, user: u64, action: &Action) -> Result<Value, Failure> {
    input::check_amount(action.amount())?;
    match action {
        Action::Cash { amount, kind } => {
            let account = accounts::repository::lock_user(conn, user)
                .await?
                .ok_or_else(accounts::service::not_found)?;
            let txid = uuid::Uuid::new_v4().to_string();
            let delta = if *kind == "deposit" {
                *amount
            } else {
                -*amount
            };
            let balance = accounts::service::move_money(conn, account, delta, kind, &txid).await?;
            Ok(json!({"transaction_id":txid,"balance":balance}))
        }
        Action::Transfer { amount, recipient } => {
            transfers::service::transfer(conn, user, recipient, *amount).await
        }
    }
}
pub async fn run(
    state: &AppState,
    ctx: &Context,
    user: u64,
    action: Action,
) -> Result<Value, AppError> {
    for attempt in 0..2 {
        let mut tx = state.db.begin().await?;
        // 커밋 오류도 재시도 판단에 포함한다. 업무 오류는 롤백한 후 실패 이벤트를 기록한다.
        let result = execute(&mut tx, user, &action).await;
        let result = match result {
            Ok(value) => tx.commit().await.map(|_| value).map_err(Failure::Db),
            Err(e) => {
                tx.rollback().await?;
                Err(e)
            }
        };
        match result {
            Ok(value) => {
                events::publish(
                    state,
                    ctx,
                    action.kind(),
                    "success",
                    "completed",
                    json!({"amount_band":events::band(action.amount())}),
                )
                .await;
                return Ok(value);
            }
            Err(Failure::Db(error)) => {
                if attempt == 0 && crate::db::error_number(&error) == Some(1213) {
                    continue;
                }
                return Err(error.into());
            }
            Err(Failure::App(error)) => {
                events::publish(
                    state,
                    ctx,
                    action.kind(),
                    "failure",
                    error.code,
                    json!({"amount_band":events::band(action.amount().max(0))}),
                )
                .await;
                return Err(error);
            }
        }
    }
    unreachable!()
}
